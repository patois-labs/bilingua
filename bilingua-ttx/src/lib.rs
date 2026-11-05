//! Bilingua TTX filter: Trados TagEditor `TRADOStag` files. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod codec;
mod tagattr;
mod tokens;
mod xml;

use bilingua_model::*;
use xml::{Kid, Kind, Tree};

/// The TTX filter. `skip_translated` gives LocOS's behaviour: a `<Tu>` whose target already holds text is not a
/// unit (it is listed as a locked unit with `include_locked`). By default such a `<Tu>` is a unit with its
/// translation loaded as the target.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ttx {
    pub skip_translated: bool,
}

impl Ttx {
    /// The filter as Localization OS reads TTX files.
    pub fn locos() -> Ttx {
        Ttx { skip_translated: true }
    }
}

pub use bilingua_model::Limits;

// Written on every `<Tu>` this pipeline translated, as LocOS: not a TM match, machine translation.
const WRITTEN_MATCH: &str = "0";
const WRITTEN_ORIGIN: &str = "mt";
const SEP: char = '\u{1f}';

// Hidden codes (ids start with `~`, never a tag id).
const RAW: &str = "~raw"; // the source span as written
const META: &str = "~meta"; // splice data, see `Meta`

fn hidden(text: impl Into<String>) -> CodeData {
    CodeData { standalone: Some(text.into()), ..Default::default() }
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.codes.get(key).and_then(|c| c.standalone.clone())
}

fn perr(msg: impl Into<String>) -> Error {
    xml::perr(msg)
}

/// TTX writes languages as upper-case `ll-CC`: LocOS `a11y.target_tag(locale).upper()`.
fn ttx_lang(code: &str) -> String {
    let t = code.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return String::new();
    }
    let parts: Vec<&str> = t.split('-').collect();
    if !(2..=8).contains(&parts[0].len()) || !parts[0].bytes().all(|c| c.is_ascii_alphabetic()) {
        return String::new();
    }
    if parts[1..].iter().any(|s| s.is_empty() || s.len() > 8 || !s.bytes().all(|c| c.is_ascii_alphanumeric())) {
        return String::new();
    }
    t.to_uppercase()
}

fn has_letter(s: &str) -> bool {
    s.chars().any(char::is_alphabetic)
}

/// `<Tu` followed by white space, `>` or `/` (the LocOS pre-parse budget counts these).
fn count_tu_opens(b: &[u8]) -> usize {
    b.windows(4).filter(|w| w.starts_with(b"<Tu") && (w[3].is_ascii_whitespace() || w[3] == b'>' || w[3] == b'/')).count()
}

fn doctype_or_entity(text: &str) -> bool {
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(p) = text[from..].find("<!") {
        let mut j = from + p + 2;
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        let rest = &b[j..];
        if (rest.len() >= 7 && rest[..7].eq_ignore_ascii_case(b"DOCTYPE")) || (rest.len() >= 6 && rest[..6].eq_ignore_ascii_case(b"ENTITY")) {
            return true;
        }
        from += p + 2;
    }
    false
}

struct Langs {
    source: String,
    target: String,
    settings: Option<usize>,
}

fn langs(tree: &Tree, text: &str) -> Langs {
    let us = tree.elems.iter().position(|e| e.kind == Kind::UserSettings);
    match us {
        None => Langs { source: String::new(), target: String::new(), settings: None },
        Some(i) => Langs {
            source: tree.attr_value(text, i, "SourceLanguage").unwrap_or_default().trim().to_string(),
            target: tree.attr_value(text, i, "TargetLanguage").unwrap_or_default().trim().to_string(),
            settings: Some(i),
        },
    }
}

/// (source Tuv, target Tuv) of a `<Tu>`.
fn pick_tuvs(tree: &Tree, text: &str, tu: usize, l: &Langs) -> (Option<usize>, Option<usize>) {
    let tuvs: Vec<usize> = tree.elems[tu]
        .kids
        .iter()
        .filter_map(|k| if let Kid::Elem(c) = k { Some(*c) } else { None })
        .filter(|c| tree.elems[*c].kind == Kind::Tuv)
        .collect();
    let Some(first) = tuvs.first().copied() else { return (None, None) };
    let lang = |t: usize| tree.attr_value(text, t, "Lang").unwrap_or_default().trim().to_lowercase();
    let src = if l.source.is_empty() { first } else { tuvs.iter().copied().find(|t| lang(*t) == l.source.to_lowercase()).unwrap_or(first) };
    let others: Vec<usize> = tuvs.iter().copied().filter(|t| *t != src).collect();
    let tgt = if l.target.is_empty() { None } else { others.iter().copied().find(|t| lang(*t) == l.target.to_lowercase()) }.or_else(|| others.first().copied());
    (Some(src), tgt)
}

/// Text runs with a letter outside every `<Tu>` and `<ut>` of the body: (start, end, value).
fn unsegmented_runs(tree: &Tree, text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let Some(body) = tree.elems[0].kids.iter().find_map(|k| match k {
        Kid::Elem(c) if tree.elems[*c].kind == Kind::Body => Some(*c),
        _ => None,
    }) else {
        return out;
    };
    fn walk(tree: &Tree, text: &str, el: usize, out: &mut Vec<(usize, usize, String)>) {
        let mut cur: Option<(usize, usize, String)> = None;
        let flush = |cur: &mut Option<(usize, usize, String)>, out: &mut Vec<(usize, usize, String)>| {
            if let Some(c) = cur.take() {
                if has_letter(&c.2) {
                    out.push(c);
                }
            }
        };
        for k in &tree.elems[el].kids {
            match *k {
                Kid::Text { s, e, cdata } => {
                    let v = if cdata { xml::line_ends(&text[s + 9..e - 3]) } else { xml::text_value(&text[s..e]) };
                    match &mut cur {
                        Some(c) => {
                            c.1 = e;
                            c.2.push_str(&v);
                        }
                        None => cur = Some((s, e, v)),
                    }
                }
                Kid::Misc => flush(&mut cur, out),
                Kid::Elem(c) => {
                    flush(&mut cur, out);
                    if !matches!(tree.elems[c].kind, Kind::Tu | Kind::Ut) {
                        walk(tree, text, c, out);
                    }
                }
            }
        }
        flush(&mut cur, out);
    }
    walk(tree, text, body, &mut out);
    out
}

fn origin_of(tu_tag: &str) -> (Option<Origin>, SegState) {
    let pct = tagattr::get(tu_tag, "MatchPercent").map(xml::text_value).and_then(|v| v.trim().parse::<u8>().ok());
    let org = tagattr::get(tu_tag, "Origin").map(xml::text_value).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let manual = org.as_deref().is_some_and(|o| o.eq_ignore_ascii_case("manual"));
    let state = if manual || pct.is_some_and(|p| p >= 100) { SegState::Translated } else { SegState::Draft };
    let kind = match org.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("manual") => Some(OriginKind::Human),
        Some("mt") => Some(OriginKind::Mt),
        Some("xtranslate" | "auto" | "tm" | "fuzzy") => Some(OriginKind::Tm),
        Some(_) => Some(OriginKind::Other),
        None if pct.is_some_and(|p| p > 0) => Some(OriginKind::Tm),
        None => None,
    };
    let origin = kind.map(|k| {
        let mut o = Origin::new(k);
        if k == OriginKind::Other {
            o.name = org.clone();
        }
        o.match_pct = pct;
        o
    });
    (origin, state)
}

struct Item {
    start: usize,
    end: usize,
    unit: Unit,
}

impl Ttx {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        guard::check_input(bytes, limits)?;
        guard::check_xml(bytes, limits)?;
        let dec = codec::decode(bytes)?;
        let text = dec.text.as_str();
        guard::check_lines(text, limits)?;
        // pre-parse budget (LocOS): bound the tree before it is built
        let cap = limits.max_units;
        let b = text.as_bytes();
        if count_tu_opens(b) > cap.saturating_mul(4) || b.iter().filter(|c| **c == b'<').count() > cap.saturating_mul(40) {
            return Err(LimitError::new(
                LimitKind::Units,
                format!("This TTX file has far more segments or tags than the limit allows ({cap} segments). Split it into smaller files."),
            )
            .into());
        }
        if doctype_or_entity(text) {
            return Err(perr("This TTX file declares a DOCTYPE or entity; TTX never needs one."));
        }
        let tree = xml::parse(text, limits)?;
        let root_name = tree.qname(text, 0);
        if !root_name.rsplit(':').next().unwrap_or("").eq_ignore_ascii_case("tradostag") {
            return Err(perr("This is not a Trados TagEditor TTX file (no <TRADOStag> root)."));
        }
        let l = langs(&tree, text);
        let mut items: Vec<Item> = Vec::new();
        let mut n_tu = 0usize;
        let mut n_locked = 0usize;
        for (idx, el) in tree.elems.iter().enumerate() {
            if el.kind != Kind::Tu || el.in_tu {
                continue;
            }
            let key = n_tu;
            n_tu += 1;
            let (src, tgt) = pick_tuvs(&tree, text, idx, &l);
            let Some(src) = src else { continue };
            let tu_raw = &text[el.start..el.end];
            let stok = tokens::tokenize(&tree, text, src);
            let ttok = tgt.map(|t| tokens::tokenize(&tree, text, t));
            let tgt_text = ttok.as_ref().is_some_and(|t| tokens::has_text(&t.inl));
            let src_text = tokens::has_text(&stok.inl);
            let locked_reason = if !src_text {
                Some("no translatable text")
            } else if tgt_text && self.skip_translated {
                Some("already translated")
            } else {
                None
            };
            if locked_reason.is_some() && !opts.include_locked {
                continue;
            }
            guard::check_unit_count(items.len() + 1, limits)?;
            let (lead, trail) = tokens::edges(&stok.inl);
            let source = tokens::strip(stok.inl.clone());
            let mut unit = match locked_reason {
                Some(_) => {
                    n_locked += 1;
                    let mut u = Unit::new(format!("k{n_locked}"), source);
                    u.translate = false;
                    u.lock_reason = locked_reason.map(str::to_string);
                    u
                }
                None => Unit::new(format!("u{}", key + 1), source),
            };
            unit.codes = stok.codes.clone();
            unit.context.kind = Some("segment".into());
            unit.context.location = Some(format!("Tu {}", key + 1));
            unit.context.key = Some(key.to_string());
            unit.codes.insert(RAW.into(), hidden(tu_raw));
            if locked_reason.is_none() {
                let tu_tag = &text[el.start..el.start_end];
                let mut sig = String::new();
                if tgt_text {
                    let t = ttok.expect("target tokens");
                    let (inl, extra) = tokens::map_target(&stok.codes, t);
                    let inl = tokens::strip(inl);
                    sig = tokens::signature(&inl);
                    let (origin, state) = origin_of(tu_tag);
                    unit.codes.extend(extra);
                    let seg = &mut unit.segments[0];
                    seg.target = inl;
                    seg.state = state;
                    seg.origin = origin;
                }
                // where a written target goes, relative to the start of the `<Tu>`
                let s_el = &tree.elems[src];
                let (kind, rs, re, nm) = match tgt {
                    Some(t) => {
                        let te = &tree.elems[t];
                        if te.empty {
                            ('s', te.start - el.start, te.end - el.start, tree.qname(text, t))
                        } else {
                            ('e', te.start_end - el.start, te.end_start - el.start, tree.qname(text, t))
                        }
                    }
                    None => ('n', s_el.end - el.start, s_el.end - el.start, tree.qname(text, src)),
                };
                let meta = [kind.to_string(), (el.start_end - el.start).to_string(), rs.to_string(), re.to_string(), nm.to_string(), lead, trail, sig].join(&SEP.to_string());
                unit.codes.insert(META.into(), hidden(meta));
            }
            items.push(Item { start: el.start, end: el.end, unit });
        }
        let runs = unsegmented_runs(&tree, text);
        let mut warnings = Vec::new();
        if n_tu == 0 {
            warnings.push(Warning::new(
                "ttx_not_segmented",
                "body",
                "This TTX file is not segmented (it has no <Tu> segments), so it cannot be translated here. Pre-translate it in Trados (Translator's Workbench, with \"Segment unknown sentences\" on) and upload the segmented TTX again.",
            ));
        }
        if !runs.is_empty() {
            warnings.push(Warning::new(
                "ttx_unsegmented_text",
                "body",
                format!("{} text run(s) outside any segment are left exactly as they are.", runs.len()),
            ));
        }
        if opts.include_locked {
            for (s, e, v) in runs {
                guard::check_unit_count(items.len() + 1, limits)?;
                n_locked += 1;
                let mut u = Unit::new(format!("k{n_locked}"), vec![Inline::text(v)]);
                u.translate = false;
                u.lock_reason = Some("text outside any segment".into());
                u.context.kind = Some("unsegmented".into());
                u.codes.insert(RAW.into(), hidden(&text[s..e]));
                items.push(Item { start: s, end: e, unit: u });
            }
        }
        items.sort_by_key(|i| i.start);
        // the UserSettings start tag is its own skeleton part, so the writer can stamp the target language
        let us_span = l.settings.map(|i| (tree.elems[i].start, tree.elems[i].start_end)).filter(|(s, _)| !items.iter().any(|it| *s >= it.start && *s < it.end));
        let mut parts = Vec::with_capacity(items.len() * 2 + 3);
        let mut units = Vec::with_capacity(items.len());
        let (mut pos, mut us_part) = (0usize, None);
        let mut us_pending = us_span;
        for it in items {
            if let Some((s, e)) = us_pending {
                if s < it.start {
                    if s > pos {
                        parts.push(SkelPart::Raw(text[pos..s].to_string()));
                    }
                    us_part = Some(parts.len());
                    parts.push(SkelPart::Raw(text[s..e].to_string()));
                    pos = e;
                    us_pending = None;
                }
            }
            if it.start > pos {
                parts.push(SkelPart::Raw(text[pos..it.start].to_string()));
            }
            pos = it.end;
            parts.push(SkelPart::Unit(units.len()));
            units.push(it.unit);
        }
        if let Some((s, e)) = us_pending {
            if s > pos {
                parts.push(SkelPart::Raw(text[pos..s].to_string()));
            }
            us_part = Some(parts.len());
            parts.push(SkelPart::Raw(text[s..e].to_string()));
            pos = e;
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let pick = |header: &str, fallback: &Option<String>| if header.is_empty() { fallback.clone() } else { Some(header.to_string()) };
        let doc = Document {
            format: "ttx".into(),
            name: None,
            source_lang: pick(&l.source, &opts.source_lang),
            target_lang: pick(&l.target, &opts.target_lang),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, codec::tag(dec.base, text, us_part), bilingua_model::parser_info!()),
            warnings: Vec::new(),
        };
        guard::finish(doc, warnings, limits)
    }
}

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return bilingua_model::encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return bilingua_model::encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(b"<\0") {
        return bilingua_model::encoding_rs::UTF_16LE.decode_without_bom_handling(head).0.into_owned();
    }
    if head.starts_with(b"\0<") {
        return bilingua_model::encoding_rs::UTF_16BE.decode_without_bom_handling(head).0.into_owned();
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

impl Filter for Ttx {
    fn id(&self) -> &'static str {
        "ttx"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ttx"]
    }

    /// Tight: XML whose first 4 KB holds a `<TRADOStag` start tag (UTF-16 or UTF-8). Position in `filters()` does not matter.
    fn sniff(&self, bytes: &[u8]) -> bool {
        head_text(bytes).to_ascii_lowercase().contains("<tradostag")
    }

    fn bilingual(&self) -> bool {
        true
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("ttx needs a text skeleton".into()));
        };
        let env = codec::parse_tag(encoding);
        let us = env.us.filter(|i| matches!(parts.get(*i), Some(SkelPart::Raw(s)) if s.starts_with('<')));
        let us_tag = us.and_then(|i| if let Some(SkelPart::Raw(s)) = parts.get(i) { Some(s.as_str()) } else { None });
        let header_target = us_tag.and_then(|t| tagattr::get(t, "TargetLanguage")).map(|v| xml::text_value(v).trim().to_string()).unwrap_or_default();
        let doc_lang = doc.target_lang.as_deref().map(ttx_lang).unwrap_or_default();
        let tgt_lang = if header_target.is_empty() { doc_lang.clone() } else { header_target.clone() };
        let mut out = String::new();
        let mut us_range = None;
        let mut touched = false;
        for (pi, p) in parts.iter().enumerate() {
            match p {
                SkelPart::Raw(s) => {
                    if Some(pi) == us {
                        us_range = Some((out.len(), out.len() + s.len()));
                    }
                    out.push_str(s);
                }
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    touched |= write_unit(u, &tgt_lang, env.crlf, &mut out)?;
                }
            }
        }
        if let (true, true, false, Some((a, b)), Some(tag)) = (touched, header_target.is_empty(), doc_lang.is_empty(), us_range, us_tag) {
            out.replace_range(a..b, &tagattr::set(tag, "TargetLanguage", &doc_lang));
        }
        codec::encode(&out, &env.base)
    }
}

/// Writes one unit; true when it changed the file.
fn write_unit(u: &Unit, tgt_lang: &str, crlf: bool, out: &mut String) -> Result<bool, Error> {
    let raw = code(u, RAW).ok_or_else(|| Error::Write(format!("unit {} lost its original markup", u.id)))?;
    let untouched = |out: &mut String| {
        out.push_str(&raw);
        Ok(false)
    };
    let Some(meta) = code(u, META).filter(|_| u.translate) else { return untouched(out) };
    let cur: Vec<Inline> = if u.segments.iter().all(|s| s.target.is_empty()) { Vec::new() } else { u.output() };
    let f: Vec<&str> = meta.split(SEP).collect();
    if f.len() != 8 || cur.is_empty() || tokens::signature(&cur) == f[7] {
        return untouched(out);
    }
    let num = |s: &str| s.parse::<usize>().map_err(|_| Error::Write(format!("unit {} has damaged splice data", u.id)));
    let (tu_len, rs, re) = (num(f[1])?, num(f[2])?, num(f[3])?);
    if tu_len > rs || rs > re || re > raw.len() || !raw.is_char_boundary(tu_len) || !raw.is_char_boundary(rs) || !raw.is_char_boundary(re) {
        return Err(Error::Write(format!("unit {} has damaged splice data", u.id)));
    }
    let Some(body) = tokens::render(&cur, &u.codes, crlf) else { return untouched(out) };
    let nl = |s: &str| if crlf { s.replace('\n', "\r\n") } else { s.to_string() };
    let inner = format!("{}{}{}", nl(f[5]), body, nl(f[6]));
    let nm = f[4];
    let (open, close) = match f[0] {
        "e" => (String::new(), String::new()),
        "s" => {
            let t = &raw[rs..re];
            (format!("{}>", t[..t.len() - 2].trim_end()), format!("</{nm}>"))
        }
        _ => {
            if tgt_lang.is_empty() {
                return Err(Error::Write(
                    "This TTX file names no target language (UserSettings TargetLanguage), so a target segment cannot be written.".into(),
                ));
            }
            (tagattr::set(&format!("<{nm}>"), "Lang", tgt_lang), format!("</{nm}>"))
        }
    };
    let seg = u.segments.first();
    let (pct, org) = match seg.and_then(|s| s.origin.as_ref()) {
        None => (WRITTEN_MATCH.to_string(), Some(WRITTEN_ORIGIN)),
        Some(o) => (
            o.match_pct.map_or(WRITTEN_MATCH.to_string(), |p| p.to_string()),
            match o.kind {
                OriginKind::Human => Some("manual"),
                OriginKind::Mt | OriginKind::Ai => Some(WRITTEN_ORIGIN),
                _ => None,
            },
        ),
    };
    let mut tag = tagattr::set(&raw[..tu_len], "MatchPercent", &pct);
    if let Some(o) = org {
        tag = tagattr::set(&tag, "Origin", o);
    }
    out.push_str(&tag);
    out.push_str(&raw[tu_len..rs]);
    out.push_str(&open);
    out.push_str(&inner);
    out.push_str(&close);
    out.push_str(&raw[re..]);
    Ok(true)
}
