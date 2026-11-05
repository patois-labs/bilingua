//! Bilingua PO filter (gettext PO and POT). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod parse;
mod ph;

use bilingua_model::*;
use parse::{escape, parse, Entry, Field};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The PO filter.
pub struct Po;

/// Reserved code ids (start with `~`, never a numeric tag id).
const RAW: &str = "raw"; // original msgstr field (plural: whole block on the first form)
const FRAW: &str = "fraw"; // original text of this form's own field
const FLAGS: &str = "fl"; // original `#,` line that holds "fuzzy"
const ENTRY: &str = "e"; // entry id, shared by all forms
const MID: &str = "mid"; // "1" when the msgid used the multi-line layout
const OV: &str = "ov"; // original msgstr value of this form (unescaped)
const FMULTI: &str = "fmulti"; // "1" when the original field used the multi-line layout
const PN: &str = "pn"; // number of plural forms of the entry

/// Plural-Forms for the languages Bilingua knows. (nplurals, expression)
fn plural_rule(lang: &str) -> Option<(usize, &'static str)> {
    let p = lang.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    match p.as_str() {
        "en" | "es" | "de" | "it" | "nl" | "pt" => Some((2, "(n != 1)")),
        "fr" => Some((2, "(n > 1)")),
        "ja" | "zh" | "ko" => Some((1, "0")),
        "ar" => Some((6, "n==0 ? 0 : n==1 ? 1 : n==2 ? 2 : n%100>=3 && n%100<=10 ? 3 : n%100>=11 ? 4 : 5")),
        _ => None,
    }
}

fn header_nplurals(header: &str) -> Option<usize> {
    let p = header.find("nplurals")?;
    let rest = header[p + 8..].trim_start().strip_prefix('=')?.trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

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
    let mut used: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for piece in ph::split(text) {
        match piece {
            ph::Piece::Text(t) => out.push(Inline::text(t)),
            ph::Piece::Ph(p) => match tags.iter().find(|(id, t)| t == p && !used.contains(id)) {
                Some((id, _)) => {
                    used.insert(id.clone());
                    out.push(placeholder_tag(id, p));
                }
                None => out.push(Inline::text(p)),
            },
        }
    }
    merge_text(out)
}

fn out_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => {
                if let Some(c) = u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                    s.push_str(c);
                }
            }
        }
    }
    s
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.hidden(key).map(str::to_string)
}

/// The text this unit writes as its msgstr ("" when no segment has a target).
fn unit_text(u: &Unit) -> String {
    if u.segments.iter().any(|s| !s.target.is_empty()) {
        out_text(u, &u.output())
    } else {
        String::new()
    }
}

/// True when the unit's text differs from the msgstr the file had.
fn changed(u: &Unit) -> bool {
    u.translate && unit_text(u) != code(u, OV).unwrap_or_default()
}

fn done(u: &Unit) -> bool {
    u.segments.iter().all(|s| !s.target.is_empty() && matches!(s.state, SegState::Translated | SegState::Confirmed))
}

impl Filter for Po {
    fn id(&self) -> &'static str {
        "po"
    }

    fn bilingual(&self) -> bool {
        true
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["po", "pot"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]);
        head.lines().any(|l| l.starts_with("msgid \"") || l.starts_with("msgctxt \""))
            && head.lines().any(|l| l.starts_with("msgstr"))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let mut dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts).declared())?;
        let text_owned = std::mem::take(&mut dec.text);
        let text = text_owned.as_str();
        let parsed = parse(text)?;

        // Header: the first entry with an empty msgid.
        let mut header_text = String::new();
        let mut header_idx = None;
        if let Some(e) = parsed.entries.first() {
            if e.ctxt.is_none() && e.id.as_ref().is_some_and(|f| f.value.is_empty()) {
                header_idx = Some(0);
                header_text = e.strs.first().map(|(_, f)| f.value.clone()).unwrap_or_default();
            }
        }
        let charset_ok = header_text
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-type:"))
            .and_then(|l| l.to_ascii_lowercase().split("charset=").nth(1).map(|c| c.trim().to_string()))
            .is_none_or(|c| c.is_empty() || c == "utf-8" || c == "utf8" || c == "ascii" || c == "us-ascii" || c == "charset");
        if !charset_ok {
            return Err(Error::Unsupported("PO files that are not UTF-8".into()));
        }
        let header_lang = header_text
            .lines()
            .find_map(|l| l.strip_prefix("Language:"))
            .map(|l| l.trim().replace('_', "-"))
            .filter(|l| !l.is_empty());
        let target_lang = opts.target_lang.clone().or(header_lang);
        let table = target_lang.as_deref().and_then(plural_rule).map(|r| r.0);
        let file_np = header_nplurals(&header_text);

        let mut units: Vec<Unit> = Vec::new();
        // (start, end, role) replaced ranges in the file; units referenced by index.
        let mut edits: Vec<(usize, usize, Vec<usize>)> = Vec::new();
        let mut header_end = 0;

        for (ei, e) in parsed.entries.iter().enumerate() {
            if Some(ei) == header_idx {
                header_end = e.strs.last().map(|(_, f)| f.end).unwrap_or(0);
                continue;
            }
            let (Some(id), false) = (&e.id, e.strs.is_empty()) else { continue };
            if id.value.is_empty() {
                continue;
            }
            entry_units(text, e, id, ei, table, file_np, &mut units, &mut edits)?;
        }
        edits.sort_by_key(|e| e.0);

        // Skeleton.
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut cursor = 0;
        if header_idx.is_some() && header_end > 0 {
            // The header entry (with the blank lines after it) is its own part.
            let mut he = header_end;
            let bytes = text.as_bytes();
            while he < bytes.len() && (bytes[he] == b'\n' || bytes[he] == b'\r') {
                he += 1;
            }
            parts.push(SkelPart::Raw(text[..he].to_string()));
            cursor = he;
        }
        for (s, e, us) in edits {
            if s < cursor {
                continue;
            }
            if s > cursor {
                parts.push(SkelPart::Raw(text[cursor..s].to_string()));
            }
            for u in us {
                parts.push(SkelPart::Unit(u));
            }
            cursor = e;
        }
        if cursor < text.len() {
            parts.push(SkelPart::Raw(text[cursor..].to_string()));
        }
        bilingua_model::guard::finish(Document::new("po", units, Skeleton::text(parts, dec.encoding.as_str(), bilingua_model::parser_info!())).with_source_lang(opts.source_lang.clone()).with_target_lang(target_lang).with_granularity(opts.granularity), std::mem::take(&mut dec.warnings), &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("po needs a text skeleton".into()));
        };
        let eol = if parts.iter().any(|p| matches!(p, SkelPart::Raw(s) if s.contains("\r\n"))) { "\r\n" } else { "\n" };
        // Which entries changed, and are all their forms done?
        let mut entry_changed: HashMap<String, bool> = HashMap::new();
        let mut entry_done: HashMap<String, bool> = HashMap::new();
        for u in &doc.units {
            if let Some(e) = code(u, ENTRY) {
                *entry_changed.entry(e.clone()).or_default() |= changed(u);
                let d = entry_done.entry(e).or_insert(true);
                *d &= done(u);
            }
        }
        let any = doc.units.iter().any(changed) || opts.stamp_lang(doc, false).is_some();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut out = String::new();
        for (pi, p) in parts.iter().enumerate() {
            match p {
                SkelPart::Raw(s) => {
                    if pi == 0 && any {
                        out.push_str(&update_header(s, doc.target_lang.as_deref(), doc));
                    } else {
                        out.push_str(s);
                    }
                }
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    let first = seen.insert(*i);
                    let entry = code(u, ENTRY).unwrap_or_default();
                    if first && u.has_hidden(FLAGS) {
                        out.push_str(&render_flags(u, entry_done.get(&entry).copied().unwrap_or(false)));
                    } else {
                        out.push_str(&render_msgstr(u, entry_changed.get(&entry).copied().unwrap_or(false), eol));
                    }
                }
            }
        }
        bilingua_model::enc::encode_text_with(&out, encoding, opts)
    }
}

#[allow(clippy::too_many_arguments)]
fn entry_units(
    text: &str,
    e: &Entry,
    id: &Field,
    ei: usize,
    table: Option<usize>,
    file_np: Option<usize>,
    units: &mut Vec<Unit>,
    edits: &mut Vec<(usize, usize, Vec<usize>)>,
) -> Result<(), Error> {
    let plural = e.id_plural.is_some();
    let first_str = &e.strs[0].1;
    let last_str = &e.strs[e.strs.len() - 1].1;
    let existing = e.strs.len();
    let bad = |msg: &str| Error::Parse { format: "po".into(), msg: format!("line {}: {msg}", id.line_no) };
    if plural && e.strs.iter().any(|(i, _)| i.is_none()) {
        return Err(bad("plural entry needs msgstr[N]"));
    }
    if !plural && (e.strs.len() != 1 || e.strs[0].0.is_some()) {
        return Err(bad("expected one msgstr"));
    }
    // Target plural count: the target language's rule, else what the entry has, else the header's.
    let n = if plural { table.or(Some(existing)).or(file_np).unwrap_or(2).max(1) } else { 1 };
    let fuzzy = e.flags.iter().any(|f| f == "fuzzy");
    let key = match &e.ctxt {
        Some(c) => format!("{} :: {}", c.value, id.value),
        None => id.value.clone(),
    };
    let mut notes: Vec<String> = e.extracted.clone();
    notes.extend(e.refs.iter().map(|r| format!("ref: {r}")));
    notes.extend(e.translator.iter().filter(|t| !t.is_empty()).map(|t| format!("comment: {t}")));
    let other_flags: Vec<String> = e.flags.iter().filter(|f| *f != "fuzzy").cloned().collect();
    let entry_id = format!("e{}", ei + 1);
    let plural_src = e.id_plural.as_ref().map(|f| f.value.as_str()).unwrap_or("");
    let mut idxs = Vec::new();
    for k in 0..n {
        let src_text = if k == 0 { id.value.as_str() } else { plural_src };
        let uid = if plural { format!("{entry_id}.{k}") } else { entry_id.clone() };
        let (src, codes, tags) = source_inlines(src_text);
        let field = e.strs.iter().find(|(i, _)| i.unwrap_or(0) == k).map(|(_, f)| f);
        let tgt_text = field.map(|f| f.value.as_str()).unwrap_or("");
        let mut u = Unit::new(uid, src);
        u.codes = codes;
        u.segments[0].target = target_inlines(tgt_text, &tags);
        u.segments[0].state = if u.segments[0].target.is_empty() {
            SegState::New
        } else if fuzzy {
            SegState::Draft
        } else {
            SegState::Translated
        };
        u.context.kind = Some("string".into());
        u.context.key = Some(key.clone());
        u.context.style = Some(if plural { format!("msgstr[{k}]") } else { "msgstr".into() });
        u.context.location = Some(format!("line {}", id.line_no));
        u.context.notes = notes.clone();
        if let Some(c) = &e.ctxt {
            u.context.extra.insert("msgctxt".into(), c.value.clone());
        }
        if plural {
            u.context.extra.insert("plural".into(), k.to_string());
            u.set_hidden(PN, n.to_string());
        }
        if fuzzy {
            u.context.extra.insert("fuzzy".into(), "true".into());
        }
        if !other_flags.is_empty() {
            u.context.extra.insert("flags".into(), other_flags.join(", "));
        }
        u.set_hidden(ENTRY, entry_id.clone());
        u.set_hidden(OV, tgt_text.to_string());
        if id.multiline() {
            u.set_hidden(MID, "1");
        }
        if let Some(f) = field {
            u.set_hidden(FRAW, text[f.start..f.end].to_string());
            if f.multiline() {
                u.set_hidden(FMULTI, "1");
            }
        }
        idxs.push(units.len());
        units.push(u);
    }
    let block = (first_str.start, last_str.end);
    units[idxs[0]].set_hidden(RAW, text[block.0..block.1].to_string());
    if let Some((fs, fe)) = e.flag_line {
        units[idxs[0]].set_hidden(FLAGS, text[fs..fe].to_string());
        edits.push((fs, fe, vec![idxs[0]]));
    }
    edits.push((block.0, block.1, idxs));
    Ok(())
}

fn render_flags(u: &Unit, all_done: bool) -> String {
    let raw = code(u, FLAGS).unwrap_or_default();
    if !all_done {
        return raw;
    }
    let (line, term) = match raw.strip_suffix("\r\n") {
        Some(l) => (l, "\r\n"),
        None => match raw.strip_suffix('\n') {
            Some(l) => (l, "\n"),
            None => (raw.as_str(), ""),
        },
    };
    let Some(body) = line.trim_start().strip_prefix("#,") else { return raw };
    let flags: Vec<&str> = body.split(',').map(|f| f.trim()).filter(|f| !f.is_empty() && *f != "fuzzy").collect();
    if flags.is_empty() {
        String::new()
    } else {
        format!("#, {}{}", flags.join(", "), term)
    }
}

fn po_string(kw: &str, text: &str, multiline: bool, eol: &str) -> String {
    let embedded = text.find('\n').is_some_and(|p| p + 1 < text.len());
    if !multiline && !embedded {
        return format!("{kw} \"{}\"", escape(text));
    }
    let mut out = format!("{kw} \"\"");
    let mut rest = text;
    while !rest.is_empty() {
        let cut = rest.find('\n').map(|p| p + 1).unwrap_or(rest.len());
        out.push_str(eol);
        out.push('"');
        out.push_str(&escape(&rest[..cut]));
        out.push('"');
        rest = &rest[cut..];
    }
    out
}

/// The text for one unit's place in the file. Untouched entries come back exactly as they were.
fn render_msgstr(u: &Unit, entry_changed: bool, eol: &str) -> String {
    let plural = u.context.extra.get("plural").and_then(|p| p.parse::<usize>().ok());
    if !entry_changed {
        return match plural {
            None | Some(0) => code(u, RAW).unwrap_or_else(|| po_string(if plural.is_some() { "msgstr[0]" } else { "msgstr" }, "", false, eol)),
            Some(_) => String::new(),
        };
    }
    let kw = match plural {
        None => "msgstr".to_string(),
        Some(k) => format!("msgstr[{k}]"),
    };
    let body = if changed(u) {
        let multiline = code(u, FMULTI).as_deref() == Some("1") || code(u, MID).as_deref() == Some("1");
        po_string(&kw, &unit_text(u), multiline, eol)
    } else {
        code(u, FRAW).unwrap_or_else(|| po_string(&kw, "", false, eol))
    };
    let n = code(u, PN).and_then(|p| p.parse::<usize>().ok()).unwrap_or(1);
    match plural {
        Some(k) if k + 1 < n => format!("{body}{eol}"),
        _ => body,
    }
}

fn update_header(raw: &str, lang: Option<&str>, doc: &Document) -> String {
    let Some(lang) = lang else { return raw.to_string() };
    if !raw.contains("msgid \"\"") {
        return raw.to_string();
    }
    let rule = plural_rule(lang);
    let has_plural = doc.units.iter().any(|u| u.context.extra.contains_key("plural"));
    let want = lang.replace('-', "_");
    let lang_value = |l: &str| {
        l.strip_prefix("\"Language:").map(|v| v.trim_start().trim_end_matches('"').trim_end_matches("\\n").trim().replace('-', "_"))
    };
    let lang_changed = raw.lines().any(|l| lang_value(l.trim_end()).is_some_and(|v| !v.eq_ignore_ascii_case(&want)));
    let mut out = String::with_capacity(raw.len());
    for line in raw.split_inclusive('\n') {
        let t = line.trim_end_matches(['\r', '\n']);
        let term = &line[t.len()..];
        if let Some(v) = t.strip_prefix("\"Language:") {
            let cur = v.trim_start().trim_end_matches('"').trim_end_matches("\\n").trim().replace('-', "_");
            if !cur.eq_ignore_ascii_case(&want) {
                out.push_str(&format!("\"Language: {want}\\n\"{term}"));
                continue;
            }
        } else if let (Some(v), Some((np, expr)), true) = (t.strip_prefix("\"Plural-Forms:"), rule, has_plural) {
            if lang_changed || header_nplurals(v) != Some(np) {
                out.push_str(&format!("\"Plural-Forms: nplurals={np}; plural={expr};\\n\"{term}"));
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests;
