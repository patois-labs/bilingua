//! Bilingua gettext MO filter (compiled message catalogs, `.mo` and `.gmo`). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod header;
mod ph;
mod plural;
mod plural_table;
mod read;
mod write;

use bilingua_model::*;
use read::{Codec, Entry};
use std::collections::{BTreeMap, HashMap};

/// The MO filter.
pub struct Mo;

/// Name of the one skeleton member that holds the file as it came in.
const MEMBER: &str = "catalog.mo";

type Source = (Vec<Inline>, BTreeMap<String, CodeData>, Vec<(String, String)>);

fn source_inlines(text: &str) -> Source {
    let mut inl = Vec::new();
    let mut codes = BTreeMap::new();
    let mut tags = Vec::new();
    for piece in ph::split(text) {
        match piece {
            ph::Piece::Text(t) => inl.push(Inline::text(t)),
            ph::Piece::Ph(p) => {
                let id = (tags.len() + 1).to_string();
                codes.insert(id.clone(), CodeData { standalone: Some(p.to_string()), label: Some("placeholder".into()), ..Default::default() });
                inl.push(placeholder_tag(&id, p));
                tags.push((id, p.to_string()));
            }
        }
    }
    (merge_text(inl), codes, tags)
}

/// A placeholder code. One that names its argument (`%1$s`, `%(name)s`, `{name}`, `{0}`) may move in the target;
/// `%s` and `{}` may not, their order is the argument order.
fn placeholder_tag(id: &str, p: &str) -> Inline {
    if bilingua_model::locos::is_named_placeholder(p) {
        Inline::free_tag(id, p)
    } else {
        Inline::tag(id, TagKind::Standalone, p)
    }
}

fn target_inlines(text: &str, tags: &[(String, String)]) -> Vec<Inline> {
    if text.is_empty() {
        return vec![];
    }
    let mut used: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for piece in ph::split(text) {
        match piece {
            ph::Piece::Text(t) => out.push(Inline::text(t)),
            ph::Piece::Ph(p) => match tags.iter().find(|(id, t)| t == p && !used.contains(&id.as_str())) {
                Some((id, _)) => {
                    used.push(id);
                    out.push(placeholder_tag(id, p));
                }
                None => out.push(Inline::text(p)),
            },
        }
    }
    merge_text(out)
}

/// The text a unit writes as its msgstr ("" when no segment has a target).
fn unit_text(u: &Unit) -> String {
    if !u.segments.iter().any(|s| !s.target.is_empty()) {
        return String::new();
    }
    let mut s = String::new();
    for i in u.output() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, display, .. } => s.push_str(u.codes.get(&id).and_then(|c| c.standalone.as_deref()).unwrap_or(&display)),
        }
    }
    s
}

fn is_message(e: &Entry) -> bool {
    !e.is_header() && !e.id.is_empty()
}

fn plural_unit(e: &Entry) -> bool {
    e.id_plural.as_deref().is_some_and(|p| !p.is_empty())
}

fn unit_id(e: &Entry, form: usize) -> String {
    if plural_unit(e) {
        format!("e{}.{form}", e.index)
    } else {
        format!("e{}", e.index)
    }
}

fn make_unit(e: &Entry, form: usize, src: &str, tgt: &str) -> Unit {
    let (inl, codes, tags) = source_inlines(src);
    let mut u = Unit::new(unit_id(e, form), inl);
    u.codes = codes;
    u.segments[0].target = target_inlines(tgt, &tags);
    u.segments[0].state = if u.segments[0].target.is_empty() { SegState::New } else { SegState::Translated };
    u.context.kind = Some("string".into());
    u.context.key = Some(match &e.ctxt {
        Some(c) => format!("{c} :: {}", e.id),
        None => e.id.clone(),
    });
    u.context.style = Some(if plural_unit(e) { format!("msgstr[{form}]") } else { "msgstr".into() });
    u.context.location = Some(format!("entry {}", e.index));
    if let Some(c) = &e.ctxt {
        u.context.extra.insert("msgctxt".into(), c.clone());
    }
    if plural_unit(e) {
        u.context.extra.insert("plural".into(), form.to_string());
    }
    u
}

/// A canonical BCP 47 tag (language lower case, script title case, region upper case), or `None` when unusable.
fn canonical_tag(lang: &str) -> Option<String> {
    let t = lang.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let mut parts = t.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in parts {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut c = sub.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

/// gettext's own rule (`msgfmt -c`): a msgstr begins and ends with a newline exactly when its msgid does.
fn edge_newlines(msgid: &str, text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let mut t = text.to_string();
    if msgid.starts_with('\n') && !t.starts_with('\n') {
        t.insert(0, '\n');
    } else if !msgid.starts_with('\n') && t.starts_with('\n') {
        t = t.trim_start_matches('\n').to_string();
    }
    if msgid.ends_with('\n') && !t.ends_with('\n') {
        t.push('\n');
    } else if !msgid.ends_with('\n') && t.ends_with('\n') {
        t = t.trim_end_matches('\n').to_string();
    }
    t
}

/// Malformed files can hold several header entries; only the real one is kept (the first whose text is made of
/// header fields, else the first), as the PO round trip does.
fn dedupe_headers(entries: &mut Vec<Entry>) {
    let heads: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].is_header()).collect();
    if heads.len() < 2 {
        return;
    }
    let keep = heads.iter().copied().find(|&i| header::looks_like_header(&entries[i].strs[0])).unwrap_or(heads[0]);
    let mut i = 0;
    entries.retain(|e| {
        let k = i;
        i += 1;
        !e.is_header() || k == keep
    });
}

fn new_header() -> Entry {
    Entry {
        index: 0,
        ctxt: None,
        id: String::new(),
        id_plural: None,
        strs: vec!["MIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: 8bit\n".into()],
    }
}

type Supplied = Vec<(Option<String>, Option<String>)>;

/// Stamps the target language and its plural rule, and fills every plural slot of that rule. `orig` holds the
/// msgstr slots of each message as the file had them, `supplied` the singular and plural text of each unit.
/// Returns true when the header was written for UTF-8 (the caller then sets the charset label to match).
fn localize(entries: &mut Vec<Entry>, orig: &[Vec<String>], supplied: &Supplied, lang: &str) -> bool {
    let Some(tag) = canonical_tag(lang) else { return false };
    let parts: Vec<&str> = tag.split('-').collect();
    let region = parts[1..].iter().find(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()));
    let value = match region {
        Some(r) => format!("{}_{r}", parts[0]),
        None => parts[0].to_string(),
    };
    let created = !entries.iter().any(Entry::is_header);
    if created {
        entries.insert(0, new_header());
    }
    let hpos = entries.iter().position(Entry::is_header).unwrap_or(0);
    let Some(rule) = plural::lookup(&value) else {
        // Unknown language: keep the declarations of an existing header; a header made just now names the target.
        if created {
            entries[hpos].strs[0].push_str(&format!("Language: {value}\n"));
            return true;
        }
        return false;
    };
    let h = &mut entries[hpos].strs[0];
    let source_language = header::set_language(h, &value);
    header::set_plural_forms(h, &rule.declaration());
    // Slots past the two source forms are kept only when the file already follows the target's language.
    let keep_extra = source_language.replace('-', "_").split('_').next() == Some(parts[0]);
    for (mi, e) in entries.iter_mut().filter(|e| is_message(e)).enumerate() {
        let (singular, plural_text) = &supplied[mi];
        let own_slots = &orig[mi];
        if e.id_plural.as_deref().is_none_or(str::is_empty) {
            continue;
        }
        e.strs.resize(rule.n, String::new());
        for i in 0..rule.n {
            let mut own = own_slots.get(i).cloned().unwrap_or_default();
            if i >= 2 && !keep_extra {
                own.clear();
            }
            let mut want = if rule.takes_singular(i) { singular.as_ref() } else { plural_text.as_ref() };
            if rule.n == 1 {
                want = plural_text.as_ref().or(singular.as_ref());
            }
            e.strs[i] = match want {
                Some(w) if !(keep_extra && !rule.owned(i) && !own.is_empty()) => edge_newlines(&e.id, w),
                _ => own,
            };
        }
    }
    true
}

/// The `(original, translation)` bytes of every entry in the codec, or `None` when a character does not fit.
fn encode_pairs(pairs: &[(String, String)], codec: Codec) -> Option<BTreeMap<Vec<u8>, Vec<u8>>> {
    let mut map = BTreeMap::new();
    for (k, v) in pairs {
        let (k, v) = (read::encode(codec, k)?, read::encode(codec, v)?);
        map.entry(k).or_insert(v); // first wins on a duplicate, as msgfmt reports it
    }
    Some(map)
}

/// Compiles the entries as Localization OS does (messages without a translation are left out, the header stays),
/// in the codec the file used when every character fits, else in UTF-8 with the charset label to match.
fn compile(entries: &[Entry], codec: Codec, full_charset: bool) -> Result<Vec<u8>, Error> {
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(entries.len());
    for e in entries {
        let (mut key, val) = match &e.id_plural {
            Some(pl) => {
                let mut slots = e.strs.clone();
                if slots.is_empty() {
                    slots = vec![String::new(); 2];
                }
                if slots.iter().all(String::is_empty) {
                    continue;
                }
                for (i, s) in slots.iter_mut().enumerate() {
                    if s.is_empty() {
                        *s = if i == 0 { e.id.clone() } else { pl.clone() };
                    }
                }
                (format!("{}\0{pl}", e.id), slots.join("\0"))
            }
            None => {
                let v = e.strs.first().cloned().unwrap_or_default();
                if !e.is_header() && v.is_empty() {
                    continue;
                }
                (e.id.clone(), v)
            }
        };
        if let Some(c) = &e.ctxt {
            key = format!("{c}\u{4}{key}");
        }
        pairs.push((key, val));
    }
    if matches!(codec, Codec::Latin1 | Codec::Legacy(_)) {
        if let Some(map) = encode_pairs(&pairs, codec) {
            return write::compile(&map);
        }
    }
    for (k, v) in pairs.iter_mut() {
        if k.is_empty() {
            let stamped = if full_charset { header::utf8_charset(v) } else { v.clone() };
            *v = header::labels_to_utf8(&stamped);
        }
    }
    let map = encode_pairs(&pairs, Codec::Utf8).ok_or_else(|| Error::Write("cannot encode the catalog".into()))?;
    write::compile(&map)
}

fn original_bytes(doc: &Document) -> Result<&[u8], Error> {
    let Skeleton::Zip { entries, .. } = &doc.skeleton else {
        return Err(Error::Write("mo needs the skeleton made by its own extract".into()));
    };
    entries
        .iter()
        .find_map(|e| match &e.data {
            ZipData::Raw(b) if e.name == MEMBER => Some(b.as_slice()),
            _ => None,
        })
        .ok_or_else(|| Error::Write("the skeleton holds no MO catalog".into()))
}

impl Filter for Mo {
    fn id(&self) -> &'static str {
        "mo"
    }

    fn bilingual(&self) -> bool {
        true
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["mo", "gmo"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        bytes.len() >= read::HEADER_BYTES && (bytes[..4] == read::LE_MAGIC.to_le_bytes() || bytes[..4] == read::BE_MAGIC.to_le_bytes())
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let cat = read::read(bytes, &limits)?;
        let mut units: Vec<Unit> = Vec::new();
        for e in cat.entries.iter().filter(|e| is_message(e)) {
            units.push(make_unit(e, 0, &e.id, e.strs.first().map_or("", String::as_str)));
            if plural_unit(e) {
                units.push(make_unit(e, 1, e.id_plural.as_deref().unwrap_or(""), e.strs.get(1).map_or("", String::as_str)));
            }
            guard::check_unit_count(units.len(), &limits)?;
        }
        let skeleton = Skeleton::zip(
            vec![ZipEntry { name: MEMBER.into(), data: ZipData::Raw(bytes.to_vec()) }],
            bilingua_model::parser_info!(),
        );
        guard::finish(
            Document {
                format: "mo".into(),
                source_lang: opts.source_lang.clone(),
                target_lang: opts.target_lang.clone(),
                granularity: opts.granularity,
                units,
                skeleton,
                ..Default::default()
            },
            vec![],
            &limits,
        )
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let original = original_bytes(doc)?;
        // The file was checked when it was extracted; read it again under limits only the file size bounds.
        let limits = Limits { max_units: usize::MAX, max_unit_chars: usize::MAX / 16, ..Limits::default() };
        let cat = read::read(original, &limits)?;
        let codec = cat.codec;
        let mut entries = cat.entries;
        dedupe_headers(&mut entries);
        let by_id: HashMap<&str, &Unit> = doc.units.iter().map(|u| (u.id.as_str(), u)).collect();

        let mut orig: Vec<Vec<String>> = Vec::new();
        let mut supplied: Supplied = Vec::new();
        let mut changes: Vec<(usize, usize, String)> = Vec::new();
        for (pos, e) in entries.iter().enumerate().filter(|(_, e)| is_message(e)) {
            let mut texts: [Option<String>; 2] = [None, None];
            for (form, slot) in texts.iter_mut().enumerate().take(if plural_unit(e) { 2 } else { 1 }) {
                let Some(u) = by_id.get(unit_id(e, form).as_str()) else { continue };
                let text = unit_text(u);
                // Only text that differs from what the file had is a translation to the target; the old msgstr of
                // an untouched unit is not (it may be in another language), so it never fills other plural slots.
                if text != e.strs.get(form).map_or("", String::as_str) {
                    changes.push((pos, form, text.clone()));
                    if !text.is_empty() {
                        *slot = Some(text);
                    }
                }
            }
            let [singular, plural_text] = texts;
            supplied.push((singular, plural_text));
            orig.push(e.strs.clone());
        }
        if changes.is_empty() {
            return Ok(original.to_vec());
        }
        for (pos, slot, text) in changes {
            let e = &mut entries[pos];
            if e.strs.len() <= slot {
                e.strs.resize(slot + 1, String::new());
            }
            e.strs[slot] = edge_newlines(&e.id, &text);
        }
        let full_charset = match doc.target_lang.as_deref().filter(|l| !l.trim().is_empty()) {
            Some(lang) => localize(&mut entries, &orig, &supplied, lang),
            None => false,
        };
        compile(&entries, codec, full_charset)
    }
}
