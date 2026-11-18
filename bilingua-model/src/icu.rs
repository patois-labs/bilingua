//! A small ICU MessageFormat parser that keeps the raw text of every piece, so an unchanged message is written
//! back exactly. Plural, selectordinal and select arguments are split into branches; `{name}` and typed
//! arguments (`{n, number}`) are simple arguments; quotes (`'{'`, `''`) stay as raw text.
//!
//! One parser for every crate that reads ICU messages (json dialects, arb; unit model step 6). It was two
//! near-copies (`bilingua-json/src/icu.rs`, `bilingua-arb/src/icu.rs`) that parsed alike; the only difference was
//! the arb copy's `locos` flag on [`translatable`] and [`has_words`], kept here as a flag (`false` is the json
//! behaviour). [`levels`] walks a parsed message the way both crates cut it into units, with the plural slot of
//! each branch and, when asked, the branches a target language needs that the source lacks.

const MAX_DEPTH: usize = 40;

/// One piece of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Raw message text (quote marks included).
    Text(String),
    /// A simple or typed argument, raw: `{name}`, `{n, number, ::compact}`.
    Simple(String),
    /// `#` inside a plural or selectordinal branch.
    Pound,
    /// A plural, selectordinal or select argument.
    Complex(Complex),
}

/// The kind of a complex argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `plural`.
    Plural,
    /// `selectordinal`.
    SelectOrdinal,
    /// `select`.
    Select,
}

impl Kind {
    /// The ICU keyword (`plural`, `selectordinal`, `select`).
    pub fn word(self) -> &'static str {
        match self {
            Kind::Plural => "plural",
            Kind::SelectOrdinal => "selectordinal",
            Kind::Select => "select",
        }
    }

    /// The plural kind of the model ([`crate::PluralKind`]).
    pub fn plural_kind(self) -> crate::PluralKind {
        match self {
            Kind::Plural => crate::PluralKind::Cardinal,
            Kind::SelectOrdinal => crate::PluralKind::Ordinal,
            Kind::Select => crate::PluralKind::Select,
        }
    }
}

/// A plural, selectordinal or select argument with its branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Complex {
    /// Raw text from `{` through the whitespace before the first selector (and an `offset:n`).
    pub head: String,
    /// Plural, selectordinal or select.
    pub kind: Kind,
    /// The branches in source order.
    pub branches: Vec<Branch>,
}

impl Complex {
    /// The argument name inside `{name, plural, ...`.
    pub fn arg(&self) -> String {
        arg_name(&self.head)
    }

    /// The first `other` branch.
    pub fn other(&self) -> Option<&Branch> {
        self.branches.iter().find(|b| b.selector == "other")
    }
}

/// One branch of a complex argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// `one`, `=0`, `female`.
    pub selector: String,
    /// Whitespace between the selector and `{`.
    pub mid: String,
    /// The branch's message.
    pub msg: Vec<Part>,
    /// Whitespace after the closing brace.
    pub trail: String,
}

/// The message is not valid ICU MessageFormat (or nests too deeply).
#[derive(Debug)]
pub struct IcuError;

type R<T> = Result<T, IcuError>;

/// Python `\s` for str patterns: Unicode white space plus the four information separators.
fn is_ws(c: char) -> bool {
    crate::py_isspace_char(c)
}

fn ch(s: &str, i: usize) -> Option<char> {
    s.get(i..).and_then(|r| r.chars().next())
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    while let Some(c) = ch(s, i) {
        if !is_ws(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// Parses a whole message. Errors on unbalanced braces, bad arguments or nesting over 40 levels.
pub fn parse(s: &str) -> R<Vec<Part>> {
    // Nothing that could be markup: one text part.
    if !s.bytes().any(|b| matches!(b, b'{' | b'}' | b'\'')) {
        return Ok(if s.is_empty() { vec![] } else { vec![Part::Text(s.to_string())] });
    }
    let (parts, i) = message(s, 0, 0, false, false)?;
    if i != s.len() {
        return Err(IcuError);
    }
    Ok(parts)
}

fn message(s: &str, mut i: usize, depth: usize, in_plural: bool, nested: bool) -> R<(Vec<Part>, usize)> {
    if depth > MAX_DEPTH {
        return Err(IcuError);
    }
    let b = s.as_bytes();
    let n = b.len();
    let mut parts = Vec::new();
    let mut run = i;
    while i < n {
        match b[i] {
            b'{' => {
                flush(s, &mut parts, run, i);
                let (p, next) = argument(s, i, depth)?;
                parts.push(p);
                i = next;
                run = i;
            }
            b'}' => {
                if !nested {
                    return Err(IcuError);
                }
                break;
            }
            b'#' if in_plural => {
                flush(s, &mut parts, run, i);
                parts.push(Part::Pound);
                i += 1;
                run = i;
            }
            b'\'' => {
                let nxt = b.get(i + 1).copied();
                if nxt == Some(b'\'') {
                    i += 2;
                } else if matches!(nxt, Some(b'{' | b'}' | b'|')) || (nxt == Some(b'#') && in_plural) {
                    let mut j = i + 1;
                    while j < n {
                        if b[j] == b'\'' {
                            if b.get(j + 1) == Some(&b'\'') {
                                j += 2;
                                continue;
                            }
                            break;
                        }
                        j += 1;
                    }
                    i = j + 1;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    let end = i.min(n);
    flush(s, &mut parts, run, end);
    Ok((parts, i))
}

fn flush(s: &str, parts: &mut Vec<Part>, from: usize, to: usize) {
    if to > from {
        parts.push(Part::Text(s[from..to].to_string()));
    }
}

fn argument(s: &str, start: usize, depth: usize) -> R<(Part, usize)> {
    let mut j = skip_ws(s, start + 1);
    // name: one or more characters that are not white space or , { } # ' |
    let name_start = j;
    while let Some(c) = ch(s, j) {
        if is_ws(c) || matches!(c, ',' | '{' | '}' | '#' | '\'' | '|') {
            break;
        }
        j += c.len_utf8();
    }
    if j == name_start {
        return Err(IcuError);
    }
    j = skip_ws(s, j);
    let b = s.as_bytes();
    match b.get(j) {
        Some(b'}') => return Ok((Part::Simple(s[start..=j].to_string()), j + 1)),
        Some(b',') => {}
        _ => return Err(IcuError),
    }
    j = skip_ws(s, j + 1);
    let ty_start = j;
    while b.get(j).is_some_and(u8::is_ascii_alphabetic) {
        j += 1;
    }
    if j == ty_start {
        return Err(IcuError);
    }
    let kind_word = &s[ty_start..j];
    j = skip_ws(s, j);
    match b.get(j) {
        Some(b'}') => return Ok((Part::Simple(s[start..=j].to_string()), j + 1)),
        Some(b',') => {}
        _ => return Err(IcuError),
    }
    let kind = match kind_word {
        "plural" => Kind::Plural,
        "selectordinal" => Kind::SelectOrdinal,
        "select" => Kind::Select,
        _ => {
            // number, date, time, ...: opaque, brace balanced, quotes skipped
            let mut level = 1usize;
            let mut k = j + 1;
            while k < b.len() && level > 0 {
                match b[k] {
                    b'{' => level += 1,
                    b'}' => level -= 1,
                    b'\'' => match s[k + 1..].find('\'') {
                        Some(p) => k = k + 1 + p,
                        None => k = b.len(),
                    },
                    _ => {}
                }
                k += 1;
            }
            if level > 0 {
                return Err(IcuError);
            }
            return Ok((Part::Simple(s[start..k].to_string()), k));
        }
    };
    j = skip_ws(s, j + 1);
    if kind != Kind::Select {
        j = offset(s, j).unwrap_or(j);
    }
    let head = s[start..j].to_string();
    let mut branches = Vec::new();
    loop {
        if j >= b.len() {
            return Err(IcuError);
        }
        if b[j] == b'}' {
            break;
        }
        let sel_end = selector_end(s, j).ok_or(IcuError)?;
        let selector = s[j..sel_end].to_string();
        let k = skip_ws(s, sel_end);
        if b.get(k) != Some(&b'{') {
            return Err(IcuError);
        }
        let (msg, k2) = message(s, k + 1, depth + 1, kind != Kind::Select, true)?;
        if b.get(k2) != Some(&b'}') {
            return Err(IcuError);
        }
        let t = skip_ws(s, k2 + 1);
        branches.push(Branch { selector, mid: s[sel_end..k].to_string(), msg, trail: s[k2 + 1..t].to_string() });
        j = t;
    }
    if branches.is_empty() {
        return Err(IcuError);
    }
    Ok((Part::Complex(Complex { head, kind, branches }), j + 1))
}

/// `offset\s*:\s*\d+\s*` at `j`: the index after it.
fn offset(s: &str, j: usize) -> Option<usize> {
    let rest = s.get(j..)?.strip_prefix("offset")?;
    let mut k = skip_ws(s, s.len() - rest.len());
    if s.as_bytes().get(k) != Some(&b':') {
        return None;
    }
    k = skip_ws(s, k + 1);
    let d0 = k;
    while s.as_bytes().get(k).is_some_and(u8::is_ascii_digit) {
        k += 1;
    }
    if k == d0 {
        return None;
    }
    Some(skip_ws(s, k))
}

/// End of a selector at `j`: `=-?\d+(\.\d+)?` first, else any run without white space or braces.
fn selector_end(s: &str, j: usize) -> Option<usize> {
    let b = s.as_bytes();
    if b.get(j) == Some(&b'=') {
        let mut k = j + 1;
        if b.get(k) == Some(&b'-') {
            k += 1;
        }
        let d0 = k;
        while b.get(k).is_some_and(u8::is_ascii_digit) {
            k += 1;
        }
        if k > d0 {
            if b.get(k) == Some(&b'.') && b.get(k + 1).is_some_and(u8::is_ascii_digit) {
                k += 1;
                while b.get(k).is_some_and(u8::is_ascii_digit) {
                    k += 1;
                }
            }
            return Some(k);
        }
    }
    let mut k = j;
    while let Some(c) = ch(s, k) {
        if is_ws(c) || c == '{' || c == '}' {
            break;
        }
        k += c.len_utf8();
    }
    (k > j).then_some(k)
}

/// True when the level has a letter or digit in its text or simple arguments (tokens do not count). With `locos`
/// a literal `⟦x<digits>⟧` in the text is a token too and does not count (LocOS `_translatable`), so a message
/// that is only such a token has no unit. (The json crate passes `false`, the arb crate its profile flag.)
pub fn translatable(parts: &[Part], locos: bool) -> bool {
    parts.iter().any(|p| match p {
        Part::Text(t) | Part::Simple(t) => has_words(t, locos),
        _ => false,
    })
}

/// A letter or digit in `t`, after removing LocOS argument tokens when `locos`.
pub fn has_words(t: &str, locos: bool) -> bool {
    const OPEN: &str = "\u{27E6}x";
    const CLOSE: char = '\u{27E7}';
    if !locos || !t.contains(OPEN) {
        return crate::py_has_alnum(t);
    }
    let mut rest = t;
    let mut kept = String::new();
    while let Some(at) = rest.find(OPEN) {
        let after = &rest[at + OPEN.len()..];
        let d = after.chars().take_while(|c| crate::py_isdecimal_char(*c)).map(char::len_utf8).sum::<usize>();
        if d > 0 && after[d..].starts_with(CLOSE) {
            kept.push_str(&rest[..at]);
            rest = &after[d + CLOSE.len_utf8()..];
        } else {
            let skip = at + '\u{27E6}'.len_utf8();
            kept.push_str(&rest[..skip]);
            rest = &rest[skip..];
        }
    }
    kept.push_str(rest);
    crate::py_has_alnum(&kept)
}

/// The argument name inside `{name, plural, ...`.
pub fn arg_name(head: &str) -> String {
    let inner = head.trim_start_matches('{').trim_start();
    inner.split([',', '}']).next().unwrap_or("").trim().to_string()
}

// ---------------------------------------------------------------------------------------------------------
// Levels and plural slots (unit model step 6)

/// Where one branch sits among the plural and select arguments of its message. Group numbers count the complex
/// arguments of one message in pre-order, from 0 (added branches included), so a crate makes a document-wide group
/// name from them and the message's first unit id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchSlot {
    /// The complex argument the branch belongs to.
    pub group: usize,
    /// The complex argument around that one, for a plural inside a select (or any nesting).
    pub parent: Option<usize>,
    /// Plural, selectordinal or select.
    pub kind: Kind,
    /// The argument name (`count`).
    pub arg: String,
    /// The selector (`one`, `=0`, `female`).
    pub category: String,
    /// A target category the source does not have, or a level inside such a branch.
    pub added: bool,
}

/// One level of a message: the message itself or a branch, with the path the crates write to `extra["icu"]`
/// (`count: plural one`, `n: plural one / g: select x`; empty for the message's own text).
#[derive(Debug, Clone)]
pub struct Level<'a> {
    /// The level's pieces.
    pub parts: &'a [Part],
    /// The path of the level in the message.
    pub path: String,
    /// The branch's slot; `None` for the message's own text.
    pub slot: Option<BranchSlot>,
}

/// The target categories `cats` that the plural or selectordinal `cx` lacks, in the order of `cats`: the
/// branches a merge for that target fills from `other`. Empty for a select and for an argument without `other`
/// (the merge fills nothing then).
pub fn missing<'c>(cx: &Complex, cats: &[&'c str]) -> Vec<&'c str> {
    if cx.kind == Kind::Select || cx.other().is_none() {
        return vec![];
    }
    cats.iter().copied().filter(|c| !cx.branches.iter().any(|b| b.selector == *c)).collect()
}

/// Every level of a parsed message in pre-order, the order the crates make units in: the level itself, then for
/// each complex argument its branches in source order, then (when `cats` gives the target categories of a kind)
/// one copy of the `other` branch for each category the argument lacks ([`missing`]), marked `added`.
pub fn levels<'a>(parts: &'a [Part], cats: &dyn Fn(Kind) -> Option<Vec<&'static str>>) -> Vec<Level<'a>> {
    let mut out = Vec::new();
    let mut groups = 0usize;
    walk(parts, String::new(), None, false, cats, &mut groups, &mut out);
    out
}

fn walk<'a>(
    parts: &'a [Part],
    path: String,
    slot: Option<BranchSlot>,
    added: bool,
    cats: &dyn Fn(Kind) -> Option<Vec<&'static str>>,
    groups: &mut usize,
    out: &mut Vec<Level<'a>>,
) {
    let parent = slot.as_ref().map(|s| s.group);
    out.push(Level { parts, path: path.clone(), slot });
    for p in parts {
        let Part::Complex(cx) = p else { continue };
        let group = *groups;
        *groups += 1;
        let arg = cx.arg();
        let here = if path.is_empty() { String::new() } else { format!("{path} / ") };
        let branch = |selector: &str, added: bool| BranchSlot { group, parent, kind: cx.kind, arg: arg.clone(), category: selector.to_string(), added };
        for b in &cx.branches {
            walk(&b.msg, format!("{here}{arg}: {} {}", cx.kind.word(), b.selector), Some(branch(&b.selector, added)), added, cats, groups, out);
        }
        if cx.kind == Kind::Select {
            continue;
        }
        let (Some(want), Some(other)) = (cats(cx.kind), cx.other()) else { continue };
        for c in missing(cx, &want) {
            walk(&other.msg, format!("{here}{arg}: {} {c}", cx.kind.word()), Some(branch(c, true)), true, cats, groups, out);
        }
    }
}

/// The hidden key ([`crate::Unit::hidden`]) of a unit added for a target category the source lacks: its ICU path
/// (`count: plural few`). A merge checks it against its own plan, so a document extracted for one language and
/// merged for another (`few` against `two`) never writes one category's text as another's.
pub const ADDED_HIDDEN: &str = "icu_added";

/// True when `unit` is what a plan expects at its place: an added unit with the same ICU path where the plan has
/// an added level (`added_path`), no added mark where it has a source level.
pub fn added_matches(unit: &crate::Unit, added_path: Option<&str>) -> bool {
    unit.hidden(ADDED_HIDDEN) == added_path
}

// ---------------------------------------------------------------------------------------------------------
// Added forms of key-style plurals (unit model step 7)

/// The [`ADDED_HIDDEN`] value of a unit added for a key-style plural form (Android `<item quantity>`, stringsdict
/// keys, i18next `_few` keys, Rails YAML keys, Fluent variants): the category and every category of the target it
/// was added for, `few of one few many other`. A merge uses the unit only when its own target categories are the
/// same list ([`key_added`]), so a document extracted for Polish and merged for Slovenian never writes Polish
/// `few` as Slovenian `few`.
pub fn key_added_mark(category: &str, cats: &[&str]) -> String {
    format!("{category} of {}", cats.join(" "))
}

/// How a merge whose target has the categories `cats` treats one unit ([`key_added`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAdded<'u> {
    /// Not an added key-style form.
    No,
    /// Added for this category of the same target categories: the merge may write it.
    Fits(&'u str),
    /// Added for another target's categories (or the merge has no target): the merge must not write it.
    Foreign,
}

/// Reads a unit's [`key_added_mark`] against the merge's target categories (`None`: the merge has no target).
pub fn key_added<'u>(unit: &'u crate::Unit, cats: Option<&[&str]>) -> KeyAdded<'u> {
    let Some((cat, list)) = unit.hidden(ADDED_HIDDEN).and_then(|m| m.split_once(" of ")) else { return KeyAdded::No };
    match cats {
        Some(cats) if list.split(' ').eq(cats.iter().copied()) && cats.contains(&cat) => KeyAdded::Fits(cat),
        _ => KeyAdded::Foreign,
    }
}

/// True when a unit has a target in some segment (a translator wrote it).
pub fn has_any_target(unit: &crate::Unit) -> bool {
    unit.segments.iter().any(|s| s.has_target())
}

/// The error a merge gives for a translated unit that was added for another target's plural categories: writing
/// it would put one language's form under another's category.
pub fn foreign_added_error(unit: &crate::Unit) -> crate::Error {
    crate::Error::TargetChanged { unit: unit.id.clone() }
}

/// The first unit of `units` added for a plural category ([`ADDED_HIDDEN`]) whose mark is not one the merge's own
/// plan adds (`fits`: the ICU paths of its added levels). A json or arb merge whose plans do not line up calls it:
/// such a unit means the document was extracted for another target's categories ([`foreign_added_error`]); with
/// none, the mismatch is a plain skeleton one.
pub fn first_foreign_added<'u>(units: &'u [crate::Unit], fits: &std::collections::HashSet<&str>) -> Option<&'u crate::Unit> {
    units.iter().find(|u| u.hidden(ADDED_HIDDEN).is_some_and(|m| !fits.contains(m)))
}

/// A copy of `other` (a fresh extract's unit) as the added form `category` of a target with the categories `cats`:
/// id `id` (its segment too), the [`key_added_mark`], and `slot` (marked added).
pub fn key_added_copy(other: &crate::Unit, id: impl Into<String>, category: &str, cats: &[&str], slot: Option<crate::PluralSlot>) -> crate::Unit {
    let mut u = other.clone();
    let id = id.into();
    let old = std::mem::take(&mut u.id);
    for s in u.segments.iter_mut().filter(|s| s.id == old) {
        s.id = id.clone();
    }
    u.id = id;
    u.set_hidden(ADDED_HIDDEN, key_added_mark(category, cats));
    u.set_plural(slot.map(|s| s.with_added(true)));
    u
}

/// Puts every unit added for a key-style plural form (an added plural slot: Android items, stringsdict keys,
/// i18next and Rails keys, Fluent variants, xcstrings forms) right after the last unit of its own group that is not
/// added (cat-os#89), so a translator meets the forms of one message together; a unit whose slot names the group as
/// its parent (a select inside a variant) counts as one of the group's units. The added units keep their order
/// (extracts make them in CLDR category order), every other unit keeps its place and every id stays as it is.
/// `parts` (the skeleton's) is pointed at the new places. An added unit whose group has no other unit stays where
/// it is. A merge must therefore find added units by their mark or id, never by position: documents from 0.10.0
/// have them at the end. ICU messages ([`levels`]) already come in this order and must not go through here: the
/// units inside an added branch belong to groups of their own.
pub fn place_added(units: &mut Vec<crate::Unit>, parts: &mut [crate::SkelPart]) {
    use std::collections::HashMap;
    let is_added = |u: &crate::Unit| u.plural().is_some_and(|p| p.is_added());
    if !units.iter().any(is_added) {
        return;
    }
    // the last unit of each group that is not added (a child group's units count for the parent)
    let mut last: HashMap<&str, usize> = HashMap::new();
    for (i, u) in units.iter().enumerate() {
        let Some(p) = u.plural().filter(|_| !is_added(u)) else { continue };
        last.insert(p.group(), i);
        if let Some(parent) = p.parent() {
            last.insert(parent, i);
        }
    }
    let mut after: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut moved = vec![false; units.len()];
    for (i, u) in units.iter().enumerate() {
        if !is_added(u) {
            continue;
        }
        if let Some(&at) = u.plural().and_then(|p| last.get(p.group())) {
            after.entry(at).or_default().push(i);
            moved[i] = true;
        }
    }
    let mut order: Vec<usize> = Vec::with_capacity(units.len());
    for i in 0..units.len() {
        if !moved[i] {
            order.push(i);
        }
        if let Some(list) = after.get(&i) {
            order.extend(list.iter().copied());
        }
    }
    if order.iter().enumerate().all(|(k, &i)| k == i) {
        return;
    }
    apply_order(units, parts, &order);
}

/// Keeps the units `order` names, in that order, and points the skeleton's unit parts at their new places (a part
/// of a unit left out keeps its number).
fn apply_order(units: &mut Vec<crate::Unit>, parts: &mut [crate::SkelPart], order: &[usize]) {
    let mut new_at = vec![usize::MAX; units.len()];
    for (k, &i) in order.iter().enumerate() {
        new_at[i] = k;
    }
    for p in parts.iter_mut() {
        if let crate::SkelPart::Unit(i) = p {
            if let Some(&n) = new_at.get(*i).filter(|n| **n != usize::MAX) {
                *i = n;
            }
        }
    }
    let mut old: Vec<Option<crate::Unit>> = std::mem::take(units).into_iter().map(Some).collect();
    units.extend(order.iter().map(|&i| old[i].take().expect("each unit once")));
}

/// For tests of format crates: `doc` with its added plural forms ([`place_added`]) moved to the end, the layout
/// 0.10.0 extracted (`keep`), or left out (`!keep`, a document from before the forms were added), the skeleton
/// pointed at the new places.
#[doc(hidden)]
pub fn reorder_added(doc: &crate::Document, keep: bool) -> crate::Document {
    let mut d = doc.clone();
    let added = |u: &crate::Unit| u.plural().is_some_and(|p| p.is_added());
    let mut order: Vec<usize> = (0..d.units.len()).filter(|&i| !added(&d.units[i])).collect();
    if keep {
        order.extend((0..d.units.len()).filter(|&i| added(&d.units[i])));
    }
    let mut none: Vec<crate::SkelPart> = Vec::new();
    let parts = match &mut d.skeleton {
        crate::Skeleton::Text { parts, .. } => parts,
        _ => &mut none,
    };
    apply_order(&mut d.units, parts, &order);
    d
}

/// Plural slots for suffix-style plural keys (ARB `itemCountOne`, `itemCountOther`): the units whose key (`key`
/// gives it, `None` for a unit that is not a whole top-level message) is a stem and a category word, grouped by
/// stem, where a stem has `Other` and one more word (the groups the merge's suffix fill reads). Group `{stem}`,
/// category the word in lower case; a unit that has a slot keeps it.
pub fn suffix_plural_slots(units: &mut [crate::Unit], key: &dyn Fn(&crate::Unit) -> Option<String>) {
    const WORDS: [&str; 6] = ["Zero", "One", "Two", "Few", "Many", "Other"];
    let mut groups: std::collections::HashMap<String, Vec<(usize, &'static str)>> = std::collections::HashMap::new();
    for (i, u) in units.iter().enumerate() {
        let Some(k) = key(u) else { continue };
        if !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        if let Some((stem, w)) = WORDS.iter().find_map(|w| k.strip_suffix(w).filter(|s| !s.is_empty()).map(|s| (s.to_string(), *w))) {
            groups.entry(stem).or_default().push((i, w));
        }
    }
    for (stem, forms) in groups {
        let mut words: Vec<&str> = forms.iter().map(|f| f.1).collect();
        words.sort_unstable();
        words.dedup();
        if words.len() < 2 || !words.contains(&"Other") {
            continue;
        }
        for (i, w) in forms {
            if units[i].plural.is_none() {
                units[i].set_plural(key_slot(stem.as_str(), false, &w.to_ascii_lowercase(), false));
            }
        }
    }
}

/// The plural slot of a key-style form: group, cardinal or ordinal, category, the `added` mark.
pub fn key_slot(group: impl Into<Box<str>>, ordinal: bool, category: &str, added: bool) -> crate::PluralSlot {
    let kind = if ordinal { crate::PluralKind::Ordinal } else { crate::PluralKind::Cardinal };
    crate::PluralSlot::new(group, kind).with_category(category).with_added(added)
}

/// A complex argument rebuilt from its rendered branches. Plural and selectordinal follow the target's
/// categories `cats`: unused ones go, missing ones take the text of `added` for that category (an added unit's
/// rendering) or else copy the `other` text, `=N` branches stay.
pub fn render_branches(cx: &Complex, rendered: Vec<String>, added: &[(&str, String)], cats: Option<&[&str]>) -> String {
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
    let cats = if cx.kind == Kind::Select { None } else { cats };
    if let Some(cats) = cats {
        if let Some(o) = list.iter().find(|b| b.selector == "other") {
            let (other_mid, other_text) = (o.mid.clone(), o.text.clone());
            let mut kept: Vec<B> = list.into_iter().filter(|b| b.selector.starts_with('=') || cats.contains(&b.selector.as_str())).collect();
            let missing: Vec<&str> = cats.iter().copied().filter(|c| !kept.iter().any(|b| b.selector == *c)).collect();
            if !missing.is_empty() {
                let idx = kept.iter().position(|b| b.selector == "other").unwrap_or(kept.len());
                let sep = if idx > 0 && !kept[idx - 1].trail.is_empty() { kept[idx - 1].trail.clone() } else { " ".to_string() };
                let new = missing.iter().map(|c| {
                    let text = added.iter().find(|(a, _)| a == c).map_or_else(|| other_text.clone(), |(_, t)| t.clone());
                    B { selector: (*c).to_string(), mid: other_mid.clone(), text, trail: sep.clone() }
                });
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

/// The model's plural slot of a branch: group names are `{message}#{group}` (`message` is the first unit id of
/// the message, `u3#0`).
pub fn plural_slot(message: &str, s: &BranchSlot) -> crate::PluralSlot {
    let mut p = crate::PluralSlot::new(format!("{message}#{}", s.group), s.kind.plural_kind()).with_category(s.category.as_str()).with_added(s.added);
    if !s.arg.is_empty() {
        p = p.with_arg(s.arg.as_str());
    }
    if let Some(g) = s.parent {
        p = p.with_parent(format!("{message}#{g}"));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(parts: &[Part]) -> String {
        let mut out = String::new();
        for p in parts {
            match p {
                Part::Text(t) | Part::Simple(t) => out.push_str(t),
                Part::Pound => out.push('#'),
                Part::Complex(cx) => {
                    out.push_str(&cx.head);
                    for b in &cx.branches {
                        out.push_str(&format!("{}{}{{{}}}{}", b.selector, b.mid, render(&b.msg), b.trail));
                    }
                    out.push('}');
                }
            }
        }
        out
    }

    fn none(_: Kind) -> Option<Vec<&'static str>> {
        None
    }

    #[test]
    fn every_byte_is_kept() {
        for s in [
            "plain",
            "Hello {name}!",
            "{n, plural, =0{none} one{# x} other{# xs}}",
            "{ count , plural , offset:1 one { # item } other { # items } }",
            "{g, select, a{A} other{B}} tail",
            "It''s '{quoted}' text",
            "{d, date, ::yMMMd} and {n, number, '{'}",
            "{a, plural, one{{b, select, x{X} other{Y}}} other{Z}}",
            "{g, select, female{{n, plural, one{# her} other{# hers}}} other{{n, plural, one{# their} other{# theirs}}}}",
            "'#' {n, plural, one{'#' # '{x}'} other{''#''}}",
        ] {
            let parts = parse(s).unwrap_or_else(|_| panic!("{s}"));
            assert_eq!(render(&parts), s);
        }
    }

    #[test]
    fn nested_select_inside_plural() {
        let parts = parse("{n, plural, one{{g, select, f{she has #} other{they have #}}} other{# items}}").unwrap();
        let Part::Complex(cx) = &parts[0] else { panic!() };
        assert_eq!((cx.kind, cx.arg(), cx.branches.len()), (Kind::Plural, "n".to_string(), 2));
        let Part::Complex(inner) = &cx.branches[0].msg[0] else { panic!() };
        assert_eq!(inner.kind, Kind::Select);
        // `#` is text in a select's branches, even inside a plural (as both crates parsed it)
        assert_eq!(inner.branches[0].msg, [Part::Text("she has #".into())]);
        let lv = levels(&parts, &none);
        let paths: Vec<&str> = lv.iter().map(|l| l.path.as_str()).collect();
        assert_eq!(paths, ["", "n: plural one", "n: plural one / g: select f", "n: plural one / g: select other", "n: plural other"]);
        let s = lv[2].slot.as_ref().unwrap();
        assert_eq!((s.group, s.parent, s.kind, s.arg.as_str(), s.category.as_str(), s.added), (1, Some(0), Kind::Select, "g", "f", false));
        assert_eq!(lv[4].slot.as_ref().unwrap().group, 0);
        assert!(lv[0].slot.is_none());
    }

    #[test]
    fn pound_exact_offsets_and_quotes() {
        // `#` is a token in a plural branch, text outside one and in a select branch
        assert_eq!(parse("# {g, select, a{#} other{#}}").unwrap().len(), 2);
        let parts = parse("{n, plural, one{#} other{##}}").unwrap();
        let Part::Complex(cx) = &parts[0] else { panic!() };
        assert_eq!(cx.branches[1].msg, [Part::Pound, Part::Pound]);
        // exact selectors, negative and decimal ones too
        let parts = parse("{n, plural, =0{none} =-1{minus} =1.5{half} other{#}}").unwrap();
        let Part::Complex(cx) = &parts[0] else { panic!() };
        let sel: Vec<&str> = cx.branches.iter().map(|b| b.selector.as_str()).collect();
        assert_eq!(sel, ["=0", "=-1", "=1.5", "other"]);
        // an offset belongs to the head, a select has none
        let parts = parse("{ n , plural , offset : 2 one{x} other{y}}").unwrap();
        let Part::Complex(cx) = &parts[0] else { panic!() };
        assert_eq!(cx.head, "{ n , plural , offset : 2 ");
        assert_eq!(cx.arg(), "n");
        assert!(parse("{g, select, offset:1 other{x}}").is_err());
        // quotes: '' is an apostrophe, '{...}' is literal, a lone ' is text
        assert_eq!(parse("It''s '{x}' ok").unwrap(), [Part::Text("It''s '{x}' ok".into())]);
        assert_eq!(parse("don't {x}").unwrap(), [Part::Text("don't ".into()), Part::Simple("{x}".into())]);
        let parts = parse("{n, plural, other{'#' is #}}").unwrap();
        let Part::Complex(cx) = &parts[0] else { panic!() };
        assert_eq!(cx.branches[0].msg, [Part::Text("'#' is ".into()), Part::Pound]);
        // typed arguments are simple, their styles opaque
        assert!(matches!(parse("{n, number, ::compact-short}").unwrap()[0], Part::Simple(_)));
        assert!(matches!(parse("{n, plural}").unwrap()[0], Part::Simple(_)));
    }

    #[test]
    fn rejects_what_localization_os_rejects() {
        let deep = format!("{}x{}", "{a, select, other{".repeat(60), "}".repeat(60));
        for bad in ["{", "}", "{n, plural, one{x}", "{n, plural}x{", "{n, plural, }", "{n, plural, one x}", "{, plural, other{x}}", "'{open", "{n, number, {x}", deep.as_str()] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_locos_flag_counts_literal_tokens_as_no_words() {
        assert!(has_words("\u{27E6}x1\u{27E7}", false));
        assert!(!has_words("\u{27E6}x1\u{27E7}", true));
        assert!(has_words("\u{27E6}x1\u{27E7} files", true));
        assert!(has_words("\u{27E6}xa\u{27E7}", true));
        assert!(!translatable(&[Part::Pound, Part::Text(" ".into())], false));
    }

    #[test]
    fn missing_categories_become_added_copies_of_other() {
        let parts = parse("{n, plural, =0{none} one{# file} other{# files}} {g, select, a{A} other{B}}").unwrap();
        let pl = |k: Kind| (k == Kind::Plural).then(|| vec!["one", "few", "many", "other"]);
        let lv = levels(&parts, &pl);
        let got: Vec<(&str, bool)> = lv.iter().map(|l| (l.path.as_str(), l.slot.as_ref().is_some_and(|s| s.added))).collect();
        assert_eq!(
            got,
            [
                ("", false),
                ("n: plural =0", false),
                ("n: plural one", false),
                ("n: plural other", false),
                ("n: plural few", true),
                ("n: plural many", true),
                ("g: select a", false),
                ("g: select other", false),
            ]
        );
        // an added copy holds the `other` branch's parts
        assert_eq!(lv[4].parts, lv[3].parts);
        // without `other` nothing is added; a select never is
        let parts = parse("{n, plural, one{x}}").unwrap();
        assert_eq!(levels(&parts, &pl).len(), 2);
    }

    #[test]
    fn render_fills_missing_categories_from_other_or_the_added_text() {
        let parts = parse("{n, plural, one{# file} other{# files}}").unwrap();
        let Part::Complex(cx) = &parts[0] else { panic!() };
        let r = || vec!["# plik".to_string(), "# plików".to_string()];
        let cats = ["one", "few", "many", "other"];
        assert_eq!(render_branches(cx, r(), &[], Some(&cats)), "{n, plural, one{# plik} few{# plików} many{# plików} other{# plików}}");
        assert_eq!(render_branches(cx, r(), &[("few", "# pliki".into())], Some(&cats)), "{n, plural, one{# plik} few{# pliki} many{# plików} other{# plików}}");
        assert_eq!(render_branches(cx, r(), &[("few", "x".into())], None), "{n, plural, one{# plik} other{# plików}}");
    }

    #[test]
    fn key_style_added_forms_fit_only_the_same_categories() {
        let other = crate::Unit::new("u3", vec![crate::Inline::text("# files")]);
        let slot = key_slot("files", false, "other", false);
        let u = key_added_copy(&other, "u3.few", "few", &["one", "few", "many", "other"], Some(slot.clone().with_category("few")));
        assert_eq!((u.id.as_str(), u.segments[0].id.as_str()), ("u3.few", "u3.few"));
        assert_eq!(u.hidden(ADDED_HIDDEN), Some("few of one few many other"));
        assert!(u.plural().is_some_and(|p| p.is_added() && p.category() == Some("few") && p.group() == "files"));
        assert_eq!(key_added(&u, Some(&["one", "few", "many", "other"])), KeyAdded::Fits("few"));
        // Slovenian has `few` too, among other categories: not the same forms
        assert_eq!(key_added(&u, Some(&["one", "two", "few", "other"])), KeyAdded::Foreign);
        assert_eq!(key_added(&u, None), KeyAdded::Foreign);
        assert_eq!(key_added(&other, Some(&["one", "other"])), KeyAdded::No);
        // an ICU added path is not a key-style mark
        let mut icu = other.clone();
        icu.set_hidden(ADDED_HIDDEN, "count: plural few");
        assert_eq!(key_added(&icu, Some(&["one", "few", "many", "other"])), KeyAdded::No);
        assert!(!has_any_target(&u));
    }

    #[test]
    fn added_forms_move_after_their_group_and_the_skeleton_follows() {
        let unit = |id: &str, group: Option<&str>, added: bool| {
            let mut u = crate::Unit::new(id, vec![crate::Inline::text(id)]);
            if let Some(g) = group {
                u.set_plural(key_slot(g, false, "other", added));
            }
            u
        };
        // the 0.10.0 layout: added units at the end
        let mut units = vec![unit("a", Some("g1"), false), unit("b", Some("g1"), false), unit("c", None, false), unit("d", Some("g2"), false), unit("b.few", Some("g1"), true), unit("b.many", Some("g1"), true), unit("d.few", Some("g2"), true), unit("x.few", Some("gone"), true)];
        let mut parts: Vec<crate::SkelPart> = (0..4).map(crate::SkelPart::Unit).collect();
        place_added(&mut units, &mut parts);
        let ids: Vec<&str> = units.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "b.few", "b.many", "c", "d", "d.few", "x.few"]);
        let at: Vec<usize> = parts.iter().map(|p| if let crate::SkelPart::Unit(i) = p { *i } else { usize::MAX }).collect();
        assert_eq!(at, [0, 1, 4, 5]);
        // and back: the test helper puts them at the end again, or leaves them out
        let doc = crate::Document::new("x", units, crate::Skeleton::text(parts, "utf-8", crate::ParserInfo::new("x", "0", 1)));
        let back = reorder_added(&doc, true);
        assert_eq!(back.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c", "d", "b.few", "b.many", "d.few", "x.few"]);
        let none = reorder_added(&doc, false);
        assert_eq!(none.units.len(), 4);
        let crate::Skeleton::Text { parts, .. } = &none.skeleton else { panic!() };
        assert!(parts.iter().enumerate().all(|(k, p)| matches!(p, crate::SkelPart::Unit(i) if *i == k)));
    }

    #[test]
    fn suffix_keys_group_by_stem() {
        let mut units: Vec<crate::Unit> = ["itemCountOne", "itemCountOther", "phone", "lonelyOther", "aOne", "aMany"].iter().map(|k| {
            let mut u = crate::Unit::new(*k, vec![]);
            u.context.key = Some((*k).to_string());
            u
        }).collect();
        suffix_plural_slots(&mut units, &|u| u.context.key.clone());
        let got: Vec<Option<(&str, &str)>> = units.iter().map(|u| u.plural().map(|p| (p.group(), p.category().unwrap()))).collect();
        assert_eq!(got, [Some(("itemCount", "one")), Some(("itemCount", "other")), None, None, None, None]);
    }

    #[test]
    fn slots_name_groups_by_message() {
        let s = BranchSlot { group: 1, parent: Some(0), kind: Kind::Select, arg: "g".into(), category: "f".into(), added: false };
        let p = plural_slot("u3", &s);
        assert_eq!((p.group(), p.parent(), p.category(), p.arg(), p.kind(), p.is_added()), ("u3#1", Some("u3#0"), Some("f"), Some("g"), crate::PluralKind::Select, false));
    }
}
