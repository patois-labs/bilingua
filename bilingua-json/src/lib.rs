//! Bilingua JSON filter.
//!
//! Every string value is a unit (`context.key` is its JSON pointer); keys are never translated. The file is not
//! re-serialised: the skeleton holds every byte between string values, and only the string token of a
//! translated value is rewritten. A value that was not translated is written back as its original token, so
//! `\/` and `é` spellings survive. Placeholders (`{name}`, `{{count}}`, `%s`, `%1$d`, `${x}`) and HTML tags
//! inside a string become inline tags. Values that are not prose (URLs, ids, numbers, ...) stay in the skeleton.
//!
//! Resource dialects (option `json.dialect`, on by default under the LocOS profile): a file shaped as Chrome
//! `messages.json`, FormatJS, Transifex Structured JSON, go-i18n or ARB gives only its message strings as units
//! (ICU plural and select branches one unit each), and a merge for a target language makes the plurals follow the
//! target's CLDR categories. Generic JSON gets i18next plural keys filled for the target (`json.i18next_plurals`).

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod cldr;
mod dialect;
mod i18next;
mod inline;
mod plan;
mod rebuild;
mod scan;
mod tree;


use bilingua_model::*;
use dialect::Dialect;

pub struct JsonFilter;

const ID: &str = "json";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// Key in `Unit::codes` that holds the original string token (a hidden code, not a tag id).
const RAW_KEY: &str = "$raw";
/// `UnitContext::extra` key naming the dialect a unit's file was read as (units of a dialect file only).
const DIALECT_KEY: &str = "json.dialect";
/// Hidden values of a unit added for an i18next plural key the target needs (unit model step 7): the new key's JSON
/// pointer, and `1` for an ordinal group.
const I18N_KEY: &str = "i18n_key";
const I18N_ORD: &str = "i18n_ord";

/// The `format_options` keys of the JSON filter.
pub const FORMAT_OPTIONS: &[bilingua_model::FormatOptionKey] = &[
    bilingua_model::FormatOptionKey::text(
        "json.dialect",
        "At extract: plain (every string is a unit; the default), auto (read the file's dialect from its shape; the default under the LocOS profile), or chrome, formatjs, structured, go, arb (the file must have that shape). A merge follows the dialect the document was read with and ignores it.",
    ),
    bilingua_model::FormatOptionKey::bool(
        "json.i18next_plurals",
        "At merge for a target language: add the CLDR plural keys the target needs to i18next-style groups (item_one, item_other) of generic JSON, holding the _other value. Default off; on under the LocOS profile, which also writes the file the way LocOS does (re-indented, two spaces).",
    ),
];

/// What `json.dialect` asks for at extract.
enum Want {
    Plain,
    Auto,
    Named(Dialect),
}

const DIALECT_WORDS: &str = "plain, auto, chrome, formatjs, structured, go or arb";

fn want_dialect(opts: &ExtractOptions, locos: bool) -> Result<Want, Error> {
    match opts.format_option("json.dialect").map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(if locos { Want::Auto } else { Want::Plain }),
        Some("plain") => Ok(Want::Plain),
        Some("auto") => Ok(Want::Auto),
        Some(n) => Dialect::parse(n)
            .map(Want::Named)
            .ok_or_else(|| Error::FormatOption { key: "json.dialect".into(), msg: format!("{n:?} is not {DIALECT_WORDS}") }),
    }
}

/// A merge accepts the same words and ignores them (the document knows its dialect).
fn check_merge_dialect(opts: &bilingua_model::MergeOptions) -> Result<(), Error> {
    match opts.format_options.get("json.dialect").map(|v| v.trim()).filter(|v| !v.is_empty()) {
        None | Some("plain" | "auto") => Ok(()),
        Some(n) if Dialect::parse(n).is_some() => Ok(()),
        Some(n) => Err(Error::FormatOption { key: "json.dialect".into(), msg: format!("{n:?} is not {DIALECT_WORDS}") }),
    }
}

impl Filter for JsonFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn format_option_keys(&self) -> &'static [bilingua_model::FormatOptionKey] {
        FORMAT_OPTIONS
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
        let Ok(text) = std::str::from_utf8(body) else { return false };
        matches!(text.trim_start().as_bytes().first(), Some(b'{' | b'[')) && scan::scan(text).is_ok()
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let mut dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts))?;
        let text_owned = std::mem::take(&mut dec.text);
        let text = text_owned.as_str();
        let encoding = dec.encoding.as_str();
        let locos = opts.is_profile(profile::LOCOS);
        // LocOS `check_json_budget` (#571 and its gate): quotes, and the commas and brackets of values that are
        // not strings, bound the work before any value is read
        let limits = opts.limits();
        let quotes = text.bytes().filter(|b| *b == b'"').count();
        bilingua_model::guard::check_budget(quotes, 40, "This JSON file has far more strings", &limits)?;
        let values = text.bytes().filter(|b| matches!(b, b',' | b'[' | b'{')).count();
        bilingua_model::guard::check_budget(values, 20, "This JSON file has far more values", &limits)?;
        let vals = scan::scan_with(text, locos, &limits)?;
        let want = want_dialect(opts, locos)?;
        if !matches!(want, Want::Plain) {
            let found = tree::scan(text).ok().and_then(|root| dialect::detect(&root).map(|d| (root, d)));
            match (want, found) {
                (Want::Named(n), Some((_, d))) if n != d => {
                    return Err(Error::FormatOption { key: "json.dialect".into(), msg: format!("the file is a {} file, not {}", d.name(), n.name()) });
                }
                (Want::Named(n), None) => {
                    return Err(Error::FormatOption { key: "json.dialect".into(), msg: format!("the file does not have the shape of a {} file", n.name()) });
                }
                (_, Some((root, d))) => {
                    let doc = extract_dialect(text, encoding, &root, d, opts, locos)?;
                    return bilingua_model::guard::finish(doc, std::mem::take(&mut dec.warnings), &opts.limits());
                }
                _ => {}
            }
        }
        // LocOS walks `json.loads`'s dict: the last of a duplicate key at the first one's place, every string
        // with a letter or digit (placeholder-only and URL values included); p_index is that walk's order
        let mut order: Vec<usize> = (0..vals.len()).filter(|&k| !vals[k].dead).collect();
        order.sort_by(|&a, &b| vals[a].ord.cmp(&vals[b].ord));
        let mut walk_at = vec![usize::MAX; vals.len()];
        let mut live = 0usize;
        for k in order {
            let (a, b) = (vals[k].start, vals[k].end);
            let alnum = scan::unescape(&text[a + 1..b - 1]).is_some_and(|v| py_has_alnum(&v));
            if alnum {
                walk_at[k] = live;
                live += 1;
            }
        }

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut pos = 0usize;
        for (k, v) in vals.into_iter().enumerate() {
            let token = &text[v.start..v.end];
            let Some(value) = scan::unescape(&token[1..token.len() - 1]) else { continue };
            let (inl, codes) = inline::tag_placeholders(&value);
            let skip = if locos { walk_at[k] == usize::MAX } else { inline::looks_untranslatable(&value, &inl) };
            if skip && (locos || !opts.include_locked || value.trim().is_empty()) {
                continue;
            }
            if v.start > pos {
                parts.push(SkelPart::Raw(text[pos..v.start].to_string()));
            }
            pos = v.end;
            let mut unit = Unit::new(format!("u{}", units.len() + 1), inl);
            unit.set_codes(codes);
            unit.codes_mut().insert(RAW_KEY, CodeData::new().with_standalone(token.to_string()));
            unit.context.kind = Some("string".into());
            unit.context.key = Some(v.pointer);
            unit.translate = !skip;
            if locos {
                unit.context.extra_mut().insert(profile::P_INDEX, walk_at[k].to_string());
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        if locos {
            // LocOS lists its units in walk order: a key repeated later stands at its first place
            in_walk_order(&mut units, &mut parts);
        } else if let Ok(root) = tree::scan(text) {
            let target = opts.target_lang.as_deref().map(str::trim).filter(|t| MergeOptions::plausible_lang(t));
            i18next_units(&root, target, &mut units, &limits)?;
        }

        bilingua_model::guard::finish(Document::new(ID, units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts), std::mem::take(&mut dec.warnings), &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        check_merge_dialect(opts)?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("json needs a text skeleton".into()));
        };
        let locos = opts.is_profile(profile::LOCOS);
        let translated = doc.units.iter().any(|u| u.segments.iter().any(|s| s.has_target()));
        let target = opts.stamp_lang(doc, translated);
        // the original text: the skeleton with every string token as it was
        let mut orig = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => orig.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    // a unit without its original token (built by hand) stands for its source, as before
                    match unit.codes().get(RAW_KEY).and_then(|c| c.standalone_markup()) {
                        Some(raw) => orig.push_str(raw),
                        None => orig.push_str(&render_inlines(unit, unit.source())),
                    }
                }
            }
        }
        if let Some(d) = doc_dialect(doc, &orig, locos) {
            let out = if !translated && target.is_none() { orig } else { rebuild::splice(&orig, doc, d, target, locos)? };
            // LocOS splices LF lines (a new `@@locale`, plural keys) into a CRLF file as they are: keep the text's breaks
            let enc = if locos { bilingua_model::enc::keep_newlines(encoding) } else { encoding.to_string() };
            return bilingua_model::enc::encode_text_with(&out, &enc, opts);
        }
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => out.push_str(&render_token(&doc.units[*i], locos)),
            }
        }
        let fill = opts.format_options.get("json.i18next_plurals").and_then(|v| match v.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None,
        });
        // units added at extract for an i18next plural key: the translated ones are written even with the fill off
        let added = i18next_added(doc, target)?;
        let on = fill.unwrap_or(locos);
        if let (true, Some(t)) = (on || !added.is_empty(), target) {
            if let Some(filled) = i18next::fill(&out, t, locos, &orig, &added, !on) {
                out = filled;
            }
        }
        bilingua_model::enc::encode_text_with(&out, encoding, opts)
    }
}

/// The dialect a document was read as: the one its units name; a document without units under the LocOS profile
/// is read again by shape.
fn doc_dialect(doc: &Document, orig: &str, locos: bool) -> Option<Dialect> {
    if let Some(n) = doc.units.iter().find_map(|u| u.context.extra().get(DIALECT_KEY)) {
        return Dialect::parse(n);
    }
    if doc.units.is_empty() && locos {
        return tree::scan(orig).ok().and_then(|r| dialect::detect(&r));
    }
    None
}

/// The document of a resource-dialect file: its message strings, ICU branches as separate units.
fn extract_dialect(text: &str, encoding: &str, root: &tree::Node, d: Dialect, opts: &ExtractOptions, locos: bool) -> Result<Document, Error> {
    let mut parts: Vec<SkelPart> = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    let mut pos = 0usize;
    // every unit of a message (an ICU message has one per branch) keeps its own copy of the pointer and the notes
    let limits = opts.limits();
    let mut context = bilingua_model::guard::ContextBudget::new();
    // outside the LocOS profile, the plural categories the target needs and a message lacks get units of their own
    let added = if locos { None } else { opts.target_lang.as_deref() };
    for m in dialect::messages(root, d) {
        let plan = plan::plan_message(m.value, m.syntax, added);
        if plan.units.is_empty() {
            continue;
        }
        if m.start > pos {
            parts.push(SkelPart::Raw(text[pos..m.start].to_string()));
        }
        pos = m.end;
        parts.push(SkelPart::Unit(units.len()));
        let first = format!("u{}", units.len() + 1);
        for (k, up) in plan.units.into_iter().enumerate() {
            let mut unit = Unit::new(format!("u{}", units.len() + 1), up.inlines);
            if let (Some(slot), false) = (&up.slot, locos) {
                unit.set_plural(bilingua_model::icu::plural_slot(&first, slot));
                if slot.added {
                    unit.set_hidden(bilingua_model::icu::ADDED_HIDDEN, up.path.clone());
                }
            }
            unit.set_codes(up.codes);
            if k == 0 {
                unit.codes_mut().insert(RAW_KEY, CodeData::new().with_standalone(text[m.start..m.end].to_string()));
            }
            unit.context.kind = Some("message".into());
            context.charge(m.pointer.len() + m.notes.iter().map(|n| n.len()).sum::<usize>(), &limits)?;
            unit.context.key = Some(m.pointer.clone());
            unit.context.notes = m.notes.to_vec();
            if !up.path.is_empty() {
                unit.context.extra_mut().insert("icu", up.path);
            }
            unit.context.extra_mut().insert(DIALECT_KEY, d.name());
            if locos {
                unit.context.extra_mut().insert(profile::P_INDEX, units.len().to_string());
            }
            units.push(unit);
        }
    }
    if pos < text.len() {
        parts.push(SkelPart::Raw(text[pos..].to_string()));
    }
    if !locos {
        // suffix-style ARB plurals and go-i18n plural objects: the forms of one message share a group (no units are
        // added for them: their merge plans every message's units and checks them one to one)
        let whole = |u: &Unit| u.context.extra().get("icu").is_none();
        match d {
            Dialect::Arb => icu::suffix_plural_slots(&mut units, &|u| u.context.key.as_deref().filter(|_| whole(u)).and_then(|k| k.strip_prefix('/')).map(str::to_string)),
            Dialect::Go => go_slots(&mut units),
            _ => {}
        }
    }
    Ok(Document::new(ID, units, Skeleton::text(parts, encoding, bilingua_model::parser_info!())).with_options(opts))
}

/// go-i18n plural objects: the units whose pointer ends in a CLDR category key, grouped by the object, where an
/// object has two or more of them, get a slot (group the object's pointer, category the key in lower case).
fn go_slots(units: &mut [Unit]) {
    let mut groups: std::collections::HashMap<String, Vec<(usize, String)>> = std::collections::HashMap::new();
    for (i, u) in units.iter().enumerate() {
        if u.plural().is_some() || u.context.extra().get("icu").is_some() {
            continue;
        }
        let Some((obj, last)) = u.context.key.as_deref().and_then(|k| k.rsplit_once('/')) else { continue };
        let cat = last.to_lowercase();
        if dialect::CLDR.contains(&cat.as_str()) {
            groups.entry(obj.to_string()).or_default().push((i, cat));
        }
    }
    for (obj, forms) in groups {
        if forms.len() < 2 {
            continue;
        }
        for (i, cat) in forms {
            units[i].set_plural(icu::key_slot(obj.as_str(), false, &cat, false));
        }
    }
}

/// i18next plural keys of generic JSON (unit model step 7): the units of one group's keys share a slot; with a
/// target, each key the plural fill adds gets a unit of its own (a copy of `_other`, id `u<n>.<category>`), which
/// the merge writes only when it is translated or the fill is on (`json.i18next_plurals`), in place of the copy.
fn i18next_units(root: &tree::Node, target: Option<&str>, units: &mut Vec<Unit>, limits: &Limits) -> Result<(), Error> {
    let groups = i18next::plural_groups(root, target);
    if groups.is_empty() {
        return Ok(());
    }
    let mut by_ptr: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, u) in units.iter().enumerate() {
        if let Some(k) = &u.context.key {
            by_ptr.entry(k.clone()).or_default().push(i);
        }
    }
    let key_ptr = |obj: &str, key: String| format!("{obj}/{}", i18next::esc(&key));
    let mut added: Vec<Unit> = Vec::new();
    for g in &groups {
        let group = key_ptr(&g.obj, g.stem.clone());
        for c in &g.cats {
            for &i in by_ptr.get(&key_ptr(&g.obj, format!("{}_{c}", g.stem))).into_iter().flatten() {
                units[i].set_plural(icu::key_slot(group.as_str(), g.ordinal, c, false));
            }
        }
        let Some(cats) = target.and_then(|t| cldr::categories(t, g.ordinal)) else { continue };
        let Some(&oi) = by_ptr.get(&key_ptr(&g.obj, format!("{}_other", g.stem))).and_then(|v| v.last()) else { continue };
        if !units[oi].translate {
            continue;
        }
        for c in &g.adds {
            let other = &units[oi];
            let mut u = icu::key_added_copy(other, format!("{}.{c}", other.id), c, &cats, other.plural().cloned().map(|s| s.with_category(*c)));
            let ptr = key_ptr(&g.obj, format!("{}_{c}", g.stem));
            u.set_hidden(I18N_KEY, ptr.clone());
            if g.ordinal {
                u.set_hidden(I18N_ORD, "1");
            }
            u.context.key = Some(ptr);
            added.push(u);
        }
    }
    bilingua_model::guard::check_unit_count(units.len() + added.len(), limits)?;
    units.extend(added);
    Ok(())
}

/// The text of the translated units added for an i18next plural key, by the key's pointer, checked against the
/// merge's target (`None`: no target): a translated unit added for another target's categories is an error.
fn i18next_added(doc: &Document, target: Option<&str>) -> Result<std::collections::HashMap<String, String>, Error> {
    let mut out = std::collections::HashMap::new();
    for u in doc.units.iter().filter(|u| u.translate && icu::has_any_target(u)) {
        let cats = target.and_then(|t| cldr::categories(t, u.has_hidden(I18N_ORD)));
        match icu::key_added(u, cats.as_deref()) {
            icu::KeyAdded::No => {}
            icu::KeyAdded::Foreign => return Err(icu::foreign_added_error(u)),
            icu::KeyAdded::Fits(_) => {
                let ptr = u.hidden(I18N_KEY).ok_or_else(|| icu::foreign_added_error(u))?;
                out.insert(ptr.to_string(), plain_inlines(u, u.output()));
            }
        }
    }
    Ok(out)
}

/// Sorts the units by their `locos.p_index` (stable), renumbers their ids and points the skeleton at the new
/// places.
fn in_walk_order(units: &mut Vec<Unit>, parts: &mut [SkelPart]) {
    let walk = |u: &Unit| u.context.extra().get(profile::P_INDEX).and_then(|p| p.parse::<usize>().ok()).unwrap_or(usize::MAX);
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|&i| walk(&units[i]));
    if order.iter().enumerate().all(|(k, &i)| k == i) {
        return;
    }
    let mut new_at = vec![0usize; units.len()];
    for (k, &i) in order.iter().enumerate() {
        new_at[i] = k;
    }
    for p in parts.iter_mut() {
        if let SkelPart::Unit(i) = p {
            *i = new_at[*i];
        }
    }
    let mut old: Vec<Option<Unit>> = std::mem::take(units).into_iter().map(Some).collect();
    for (k, &i) in order.iter().enumerate() {
        let mut u = old[i].take().expect("each unit once");
        let was = std::mem::replace(&mut u.id, format!("u{}", k + 1));
        for s in u.segments.iter_mut().filter(|s| s.id == was) {
            s.id = u.id.clone();
        }
        units.push(u);
    }
}

/// The string token to write for a unit. Under the LocOS profile a translation equal to the source keeps the
/// original token too, as LocOS's splice does.
fn render_token(unit: &Unit, locos: bool) -> String {
    let untouched = unit.segments.iter().all(|s| !s.has_target()) || (locos && unit.output() == unit.source());
    if untouched {
        if let Some(raw) = unit.codes().get(RAW_KEY).and_then(|c| c.standalone_markup()) {
            return raw.to_string();
        }
    }
    render_inlines(unit, unit.output())
}

/// The JSON string token of `inl`, the unit's tags written back as their codes.
fn render_inlines(unit: &Unit, inl: Vec<Inline>) -> String {
    scan::quote(&plain_inlines(unit, inl))
}

/// The text of `inl`, the unit's tags written back as their codes.
fn plain_inlines(unit: &Unit, inl: Vec<Inline>) -> String {
    let mut text = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => text.push_str(&t),
            Inline::Tag { id, kind, .. } => {
                let code: Option<&CodeData> = unit.codes().get(&id);
                let piece = code.and_then(|c| match kind {
                    TagKind::Open => c.open_markup(),
                    TagKind::Close => c.close_markup(),
                    TagKind::Standalone => c.standalone_markup(),
                });
                if let Some(p) = piece {
                    text.push_str(p);
                }
            }
        }
    }
    text
}

/// The resource dialect a UTF-8 JSON file has by its shape (`chrome`, `formatjs`, `structured`, `go`, `arb`), the
/// one `json.dialect=auto` would read it as; `None` for a file that is not JSON or has no dialect shape. The facade
/// uses it to route an ARB-looking `.json` the way LocOS does (a go-i18n file with `@@locale` is go-i18n).
pub fn dialect_of(bytes: &[u8]) -> Option<&'static str> {
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let text = std::str::from_utf8(body).ok()?;
    tree::scan(text).ok().and_then(|root| dialect::detect(&root)).map(Dialect::name)
}

/// Exposed for tests and tools: the JSON pointers of all units, in order.
pub fn pointers(doc: &Document) -> Vec<String> {
    doc.units.iter().filter_map(|u| u.context.key.clone()).collect()
}

#[cfg(test)]
mod tests;
