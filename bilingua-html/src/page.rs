//! HTML to [`Document`] and back, on the LocOS engine in `bilingua-htmlcore` (LocOS `HtmlFilter.parse`, `build`
//! and `build_localized`): block units with inline tags, then the accessibility attributes, then (with
//! `page_meta`) the page metadata. The skeleton is the page itself; merge re-derives the units from it and writes
//! only the translated ones, so with nothing translated it returns the input byte for byte.

use bilingua_htmlcore::browser;
use bilingua_htmlcore::html::{self, Entry, Piece, Tok, K};
use bilingua_htmlcore::pyx::*;
use bilingua_model::*;
use regex::Regex;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Hidden value that marks an attribute unit (`v` = `attr`).
pub const VALUE: &str = "v";

/// What one filter instance is (LocOS `HtmlFilter(template_pattern, page_meta)`).
#[derive(Clone, Copy)]
pub(crate) struct Cfg<'a> {
    pub page_meta: bool,
    pub template: Option<&'a Regex>,
}

impl Cfg<'_> {
    /// LocOS `_server_page`: only a plain HTML filter with no template pattern routes server code.
    fn routes(&self) -> bool {
        self.template.is_none()
    }
}

const SERVER_PAGES: &str = "bilingua-serverpages";

fn too_deep(l: &Limits) -> Error {
    LimitError::new(LimitKind::Depth, format!("This HTML file is nested more than {} levels deep. It was not processed.", l.max_xml_depth)).into()
}

/// Elements whose end tag may be left out: a run of them is siblings, not nesting.
const OPTIONAL_END: [&str; 16] =
    ["p", "li", "dt", "dd", "tr", "td", "th", "option", "optgroup", "thead", "tbody", "tfoot", "colgroup", "caption", "rt", "rp"];

const VOID: [&str; 18] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr", "basefont", "bgsound", "frame",
    "keygen",
];

/// Refuses markup nested deeper than `Limits::max_xml_depth` (Bilingua's own cap; LocOS has none): a start tag opens
/// a level unless it is void, self-closing or end-tag-optional; an end tag closes one only when an element of its
/// name is open. One pass over the tokens.
fn check_depth(toks: &[Tok], l: &Limits) -> Result<(), Error> {
    let mut depth = 0usize;
    let mut open: HashMap<String, usize> = HashMap::new();
    let mut skip = String::new();
    for t in toks {
        if t.k != K::Markup || t.s.starts_with("<!") || t.s.starts_with("<?") {
            continue;
        }
        let (closing, name, sc) = html::parse_tag_str(&t.s);
        if name.is_empty() {
            continue;
        }
        if !skip.is_empty() {
            if closing && name == skip {
                skip.clear();
            }
            continue;
        }
        if closing {
            if let Some(n) = open.get_mut(&name).filter(|n| **n > 0) {
                *n -= 1;
                depth = depth.saturating_sub(1);
            }
        } else if !sc && !VOID.contains(&name.as_str()) && !OPTIONAL_END.contains(&name.as_str()) {
            depth += 1;
            if depth > l.max_xml_depth {
                return Err(too_deep(l));
            }
            *open.entry(name).or_default() += 1;
        }
    }
    Ok(())
}

fn label_for(name: &str) -> String {
    match name {
        "b" | "strong" => "bold",
        "i" | "em" | "cite" => "italic",
        "a" => "link",
        "u" | "ins" => "underline",
        "s" | "strike" | "del" => "strikethrough",
        "code" | "kbd" | "samp" | "tt" => "code",
        "br" | "wbr" => "line break",
        "img" => "image",
        "sup" => "superscript",
        "sub" => "subscript",
        "abbr" => "abbreviation",
        "mark" => "highlight",
        other => other,
    }
    .to_string()
}

fn kind_for(frame: &str) -> &'static str {
    match frame {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
        "li" | "dt" | "dd" => "list-item",
        "td" | "th" => "cell",
        "button" => "button",
        "label" => "label",
        "option" => "option",
        "title" => "title",
        "figcaption" | "caption" => "caption",
        "summary" => "summary",
        "textarea" => "label",
        _ => "paragraph",
    }
}

fn attr_kind(style: &str) -> &'static str {
    if style.ends_with("alt") {
        "alt"
    } else if style.ends_with("title") && style.starts_with("a11y") {
        "title"
    } else if style.ends_with("aria-label") {
        "label"
    } else {
        "meta"
    }
}

/// Where a token sits: the innermost open block element (its name and start-token index) and a CSS-like path.
#[derive(Clone, Default)]
pub(crate) struct Place {
    pub elem: String,
    pub elem_ti: Option<usize>,
    pub path: String,
}

/// The place of every token in `wanted`, from one walk over the token stream (end-tag-optional siblings close each
/// other as a browser would; inline elements are not places).
pub(crate) fn places(toks: &[Tok], wanted: &HashSet<usize>) -> HashMap<usize, Place> {
    struct Frame {
        name: String,
        nth: usize,
        ti: usize,
        counts: HashMap<String, usize>,
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut root: HashMap<String, usize> = HashMap::new();
    let mut out = HashMap::new();
    // the innermost 32 levels: a path is a hint for the translator, and a page of 20 000 unclosed `<li><dd>`
    // must not cost a path per level per unit
    let path = |stack: &[Frame]| {
        let from = stack.len().saturating_sub(32);
        let p = stack[from..].iter().filter(|f| f.name != "html").map(|f| format!("{}:nth-of-type({})", f.name, f.nth)).collect::<Vec<_>>().join(" > ");
        if from > 0 {
            format!("\u{2026} > {p}")
        } else {
            p
        }
    };
    for (ti, t) in toks.iter().enumerate() {
        if wanted.contains(&ti) {
            let top = stack.last();
            out.insert(ti, Place { elem: top.map(|f| f.name.clone()).unwrap_or_default(), elem_ti: top.map(|f| f.ti), path: path(&stack) });
        }
        if t.k != K::Markup || t.s.starts_with("<!") || t.s.starts_with("<?") {
            continue;
        }
        let (closing, name, sc) = html::parse_tag_str(&t.s);
        if name.is_empty() || html::is_inline_tag(&name) {
            continue;
        }
        if closing {
            if let Some(p) = stack.iter().rposition(|f| f.name == name) {
                stack.truncate(p);
            }
            continue;
        }
        let siblings: &[&str] = match name.as_str() {
            "li" => &["li", "p"],
            "dt" | "dd" => &["dt", "dd", "p"],
            "td" | "th" => &["td", "th", "p"],
            "tr" => &["tr", "td", "th", "p"],
            "option" => &["option"],
            "optgroup" => &["optgroup", "option"],
            "thead" | "tbody" | "tfoot" => &["thead", "tbody", "tfoot", "tr", "td", "th"],
            n if html::closes_p(n) => &["p"],
            _ => &[],
        };
        while stack.last().is_some_and(|f| siblings.contains(&f.name.as_str())) {
            stack.pop();
        }
        if sc || VOID.contains(&name.as_str()) {
            continue;
        }
        let counts = match stack.last_mut() {
            Some(f) => &mut f.counts,
            None => &mut root,
        };
        let c = counts.entry(name.clone()).or_insert(0);
        *c += 1;
        let nth = *c;
        stack.push(Frame { name, nth, ti, counts: HashMap::new() });
    }
    out
}

/// LocOS's unit text of a body unit (`core`): text as typed, inline tags as `⟦g1⟧ ⟦/g1⟧ ⟦x2⟧` tokens.
fn core_text(u: &html::BodyUnit) -> String {
    let mut s = String::new();
    for p in &u.parts {
        match p {
            Piece::Text(t) => s.push_str(t),
            Piece::Open(n) => s.push_str(&format!("\u{27E6}g{n}\u{27E7}")),
            Piece::Close(n) => s.push_str(&format!("\u{27E6}/g{n}\u{27E7}")),
            Piece::Point(n) => s.push_str(&format!("\u{27E6}x{n}\u{27E7}")),
        }
    }
    s
}

fn body_unit(id: String, u: &html::BodyUnit) -> Unit {
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes = BTreeMap::new();
    for p in &u.parts {
        match p {
            Piece::Text(t) => inl.push(Inline::text(*t)),
            Piece::Open(n) | Piece::Close(n) | Piece::Point(n) => {
                let e = &u.map[*n as usize - 1];
                let key = n.to_string();
                match p {
                    Piece::Open(_) => {
                        inl.push(Inline::tag(key.clone(), TagKind::Open, format!("<{}>", e.name)));
                        codes.insert(key, CodeData::new().with_open(e.open.clone()).with_close(e.close.clone()).with_label(label_for(&e.name)));
                    }
                    Piece::Close(_) => inl.push(Inline::tag(key, TagKind::Close, format!("</{}>", e.name))),
                    _ => {
                        let raw = e.point.clone().unwrap_or_default();
                        let (display, label) = if e.name.is_empty() {
                            let mut d: String = raw.chars().take(80).collect();
                            if raw.chars().count() > 80 {
                                d.push('\u{2026}');
                            }
                            (d, "not translated".to_string())
                        } else {
                            (format!("<{}>", e.name), label_for(&e.name))
                        };
                        inl.push(Inline::tag(key.clone(), TagKind::Standalone, display));
                        codes.insert(key, CodeData::new().with_standalone(raw).with_label(label));
                    }
                }
            }
        }
    }
    let mut unit = Unit::new(id, merge_text(inl));
    unit.codes = codes;
    unit
}

/// LocOS `read_text_upload(declared=True)`, plus Bilingua's rule that an untouched merge gives the input back: a
/// legacy encoding whose bytes would not be written back the same is refused.
fn load(bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<enc::Decoded, Error> {
    guard::check_input(bytes, limits)?;
    let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()).declared())?;
    if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && enc::encode_text(&dec.text, &dec.encoding)? != bytes {
        return Err(Error::Encoding(
            "This file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into(),
        ));
    }
    Ok(dec)
}

fn units_limit(limits: &Limits) -> Error {
    LimitError::new(
        LimitKind::Units,
        format!("This HTML file has far more tags than {} segments allow. Split it into smaller files.", limits.max_units),
    )
    .into()
}

/// The language of the page (`<html lang>`), for the source language when the caller gave none.
fn page_lang(toks: &[Tok]) -> Option<String> {
    let (ti, _) = html::iter_start_tags(toks, false).into_iter().find(|(_, n)| n == "html")?;
    let attrs = html::tag_attr_values(&toks[ti].s);
    attrs.into_iter().find(|(k, _)| k == "lang").map(|(_, v)| trim(&v).to_string()).filter(|v| !v.is_empty())
}

/// The name of the quoted attribute whose value starts at `start` in token `ti`. The attributes of a token are read
/// once (`seen`): a tag of thousands of `alt`s must not be rescanned per attribute unit.
fn attr_name(seen: &mut HashMap<usize, HashMap<usize, String>>, ti: usize, tok: &str, start: usize) -> String {
    seen.entry(ti).or_insert_with(|| html::tag_attrs(tok).into_iter().map(|a| (a.1, a.0)).rev().collect()).get(&start).cloned().unwrap_or_default()
}

pub(crate) fn extract(bytes: &[u8], opts: &ExtractOptions, cfg: Cfg) -> Result<Document, Error> {
    let limits = opts.limits();
    let dec = load(bytes, opts, &limits)?;
    let text = dec.text.as_str();
    if cfg.routes() {
        if let Some(d) = bilingua_serverpages::server_page_for_html(text)? {
            // LocOS #672: a .html page with closed server code is read as that server page
            let mut doc = d.filter().extract(bytes, opts)?;
            doc.format = "html".into();
            return Ok(doc);
        }
    }
    // LocOS #571: ten markup characters per allowed segment, and a hundred tag-scan steps per allowed segment
    if text.bytes().filter(|&c| c == b'<').count() > limits.max_units.saturating_mul(10) {
        return Err(units_limit(&limits));
    }
    guard::check_markup(text, &limits)?;
    let mut budget = Some(i64::try_from(limits.max_units).unwrap_or(i64::MAX).saturating_mul(100));
    let toks = html::tokenize_with(text, cfg.template, &mut budget).map_err(|_| units_limit(&limits))?;
    check_depth(&toks, &limits)?;
    let body = match html::iter_units(&toks, Some(limits.max_units)) {
        Ok(b) => b,
        Err(n) => {
            guard::check_unit_count(n, &limits)?;
            Vec::new()
        }
    };
    let attrs = html::iter_page_attr_units_with(&toks, cfg.page_meta);
    let wanted: HashSet<usize> = body.iter().map(|u| u.start).chain(attrs.iter().map(|a| a.ti)).collect();
    let place = places(&toks, &wanted);
    let mut units: Vec<Unit> = Vec::with_capacity(body.len() + attrs.len());
    let mut heading: Option<String> = None;
    let mut attr_names: HashMap<usize, HashMap<usize, String>> = HashMap::new();
    for (i, u) in body.iter().enumerate() {
        let mut unit = body_unit(format!("u{}", i + 1), u);
        let p = place.get(&u.start).cloned().unwrap_or_default();
        let kind = kind_for(&p.elem);
        unit.context.kind = Some(kind.into());
        unit.context.location = Some(p.path).filter(|s| !s.is_empty());
        if kind == "heading" {
            heading = Some(plain_text(&unit.source()));
        } else {
            unit.context.heading = heading.clone();
        }
        units.push(unit);
    }
    for a in &attrs {
        guard::check_unit_count(units.len() + 1, &limits)?;
        let tok = &toks[a.ti].s;
        let mut u = Unit::new(format!("u{}", units.len() + 1), vec![Inline::text(a.text.clone())]);
        let elem = html::parse_tag_str(tok).1;
        let attr = attr_name(&mut attr_names, a.ti, tok, a.start);
        let p = place.get(&a.ti).cloned().unwrap_or_default();
        let own = if p.path.is_empty() { elem.clone() } else { format!("{} > {elem}", p.path) };
        u.context.kind = Some(attr_kind(&a.style).into());
        u.context.style = Some(a.style.clone());
        u.context.location = Some(format!("{own}[{attr}]"));
        u.context.key = Some(attr);
        u.context.heading = heading.clone();
        u.context.extra.insert("element".into(), elem);
        u.set_hidden(VALUE, "attr");
        units.push(u);
    }
    let source_lang = opts.source_lang.clone().or_else(|| page_lang(&toks));
    let doc = Document::new("html", units, Skeleton::text(vec![SkelPart::Raw(text.to_string())], dec.encoding.clone(), parser_info!()))
        .with_options(opts)
        .with_source_lang(source_lang);
    guard::finish(doc, dec.warnings, &limits)
}

/// The page of a document's skeleton.
fn skeleton_text(doc: &Document) -> Result<(String, &str), Error> {
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("html needs a text skeleton".into()));
    };
    let mut text = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => text.push_str(s),
            _ => return Err(Error::Write("this HTML skeleton comes from an older Bilingua; open the file again".into())),
        }
    }
    Ok((text, encoding))
}

/// A target the page writes: the unit's output when it is translated and differs from the source.
fn target_of(u: &Unit) -> Option<Vec<Inline>> {
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    let out = u.output();
    (out != u.source()).then_some(out)
}

/// LocOS `template_pattern.findall(core)`: the whole match, or group 1 of a pattern with groups.
fn protected_of(template: Option<&Regex>, core: &str) -> Vec<String> {
    let Some(re) = template else { return Vec::new() };
    if re.captures_len() > 1 {
        re.captures_iter(core).map(|c| c.get(1).map_or(String::new(), |m| m.as_str().to_string())).collect()
    } else {
        re.find_iter(core).map(|m| m.as_str().to_string()).collect()
    }
}

/// LocOS `_rematerialize`: text escaped (quotes too when `quotes`), template spans kept, every inline code the
/// original markup of its token; a code the unit does not have is dropped.
pub(crate) fn materialize(target: &[Inline], map: &[Entry], quotes: bool, protected: &[String]) -> String {
    let mut out = String::new();
    for i in target {
        match i {
            Inline::Text(t) => escape_protected(t, protected, quotes, &mut out),
            Inline::Tag { id, kind, .. } => {
                let Some(e) = id.parse::<usize>().ok().filter(|n| *n >= 1).and_then(|n| map.get(n - 1)) else { continue };
                let code = match kind {
                    TagKind::Standalone => e.point.as_deref(),
                    TagKind::Open => e.open.as_deref(),
                    TagKind::Close => e.close.as_deref(),
                };
                out.push_str(code.unwrap_or(""));
            }
        }
    }
    out
}

/// The page tokens with the translated attribute values written in (LocOS `_apply_attr_units`). Returns the
/// number of body units.
pub(crate) fn apply_attrs(toks: &mut [Tok], doc: &Document, page_meta: bool, values: impl Fn(&Unit) -> Option<String>) -> Result<usize, Error> {
    let n_body = html::iter_units(toks, None).map_err(|_| Error::Write("unit count".into()))?.len();
    let attrs = html::iter_page_attr_units_with(toks, page_meta);
    if doc.units.len() != n_body + attrs.len() {
        return Err(Error::Write(format!("the document has {} units but its page holds {}; open the file again", doc.units.len(), n_body + attrs.len())));
    }
    let mut edits: BTreeMap<usize, Vec<(usize, usize, String)>> = BTreeMap::new();
    let mut strict: Option<browser::Strict> = None;
    for (j, a) in attrs.iter().enumerate() {
        let Some(new) = values(&doc.units[n_body + j]) else { continue };
        if new == a.text {
            continue;
        }
        let strict = strict.get_or_insert_with(|| browser::attr_strictness(toks));
        let mut val = html_escape(&new);
        if strict(a.ti, a.start, a.end) {
            val = strict_escape(&val);
        }
        edits.entry(a.ti).or_default().push((a.start, a.end, val));
    }
    for (ti, mut spans) in edits {
        // one pass over the token (a replace per value would move the rest of a tag of thousands of values each time)
        spans.sort_by_key(|x| x.0);
        let old = toks[ti].s.to_string();
        let mut tok = String::with_capacity(old.len());
        let mut last = 0;
        for (s, e, v) in spans {
            tok.push_str(&old[last..s]);
            tok.push_str(&v);
            last = e;
        }
        tok.push_str(&old[last..]);
        toks[ti].s = Cow::Owned(tok);
    }
    Ok(n_body)
}

/// True when the page declares a charset (`;label=`) other than the code page it was read in (LocOS reads
/// such a page with its Windows-1252 fallback, as Bilingua does).
fn misdeclared(tag: &str) -> bool {
    use bilingua_model::encoding_rs::Encoding;
    let base = enc::base(tag);
    if matches!(base, "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") {
        return false;
    }
    let Some(label) = tag.split(';').find_map(|p| p.strip_prefix("label=")) else { return false };
    match (Encoding::for_label(base.as_bytes()), Encoding::for_label(label.trim().as_bytes())) {
        (Some(read), Some(said)) => read != said,
        _ => false,
    }
}

/// A document read through server-page routing: merge goes back through that dialect.
fn server_dialect(doc: &Document, text: &str) -> Result<Option<bilingua_serverpages::Dialect>, Error> {
    if doc.skeleton.parser().is_none_or(|p| p.name != SERVER_PAGES) {
        return Ok(None);
    }
    bilingua_serverpages::server_page_for_html(text)?
        .map(Some)
        .ok_or_else(|| Error::Write("this document was read as a server page, but its page holds no server code; open the file again".into()))
}

pub(crate) fn merge(doc: &Document, opts: &MergeOptions, cfg: Cfg) -> Result<Vec<u8>, Error> {
    // a schema 1 document with units has the units of the old filter (other ids and order): refused with
    // `Error::ParserVersion` before its skeleton is read; one without units (a page-only skeleton) still merges
    if doc.skeleton.parser().is_none_or(|p| p.name != SERVER_PAGES) {
        check_parser_min(doc, &parser_info!(), if doc.units.is_empty() { 1 } else { crate::SKELETON_SCHEMA })?;
    }
    let (text, encoding) = skeleton_text(doc)?;
    if let Some(d) = server_dialect(doc, &text)? {
        return d.filter().merge_with(doc, opts);
    }
    let targets: Vec<Option<Vec<Inline>>> = doc.units.iter().map(target_of).collect();
    let translated = targets.iter().any(Option::is_some);
    let tag = opts.stamp_lang(doc, translated).and_then(html::html_lang);
    if !translated && tag.is_none() {
        return enc::encode_text_with(&text, encoding, opts);
    }
    let mut toks = html::tokenize_with(&text, cfg.template, &mut None).map_err(|_| Error::Write("token budget".into()))?;
    if let Some(tag) = &tag {
        // LocOS build_localized: declared on the original token stream, before any translation is written
        html::declare_language(&mut toks, tag, html::text_direction(tag));
    }
    let before: String = toks.iter().map(|t| t.s.as_ref()).collect();
    // LocOS `encode_like`: a page read in another code page than the one it declares goes out as UTF-8 (its
    // declaration rewritten), not in the code page it was read in under a declaration that says otherwise
    let utf8;
    let opts = if misdeclared(encoding) {
        utf8 = opts.clone().with_output_encoding(OutputEncoding::Utf8);
        &utf8
    } else {
        opts
    };
    // #672: quotes in text stay raw only where the page parses the same in a browser, and never with templates
    let quotes = cfg.template.is_some() || !browser::raw_quotes_safe(&before);
    let n_body = apply_attrs(&mut toks, doc, cfg.page_meta, |u| target_of(u).map(|t| plain_text(&t)))?;
    let mut repl: Vec<(usize, usize, String)> = Vec::new();
    {
        let body = html::iter_units(&toks, None).map_err(|_| Error::Write("unit count".into()))?;
        if body.len() != n_body {
            return Err(Error::Write("the page changed shape while its attributes were written".into()));
        }
        for (k, u) in body.iter().enumerate() {
            if let Some(t) = &targets[k] {
                let protected = protected_for(cfg, u);
                repl.push((u.start, u.end, format!("{}{}{}", u.lead, materialize(t, &u.map, quotes, &protected), u.trail)));
            }
        }
    }
    for (s, e, v) in repl {
        toks[s].s = Cow::Owned(v);
        for t in &mut toks[s + 1..=e] {
            t.s = Cow::Borrowed("");
        }
    }
    let out: String = toks.iter().map(|t| t.s.as_ref()).collect();
    // #676: a browser-equivalent re-parse (and an XML one for XHTML) must find only the source's elements
    if before != out {
        let sources = browser::browser_views(&before);
        if let Err(u) = browser::check_build(&before, &out, &sources, true) {
            return Err(Error::Write(u.message()));
        }
    }
    enc::encode_text_with(&out, encoding, opts)
}

/// The page tokens for the preview (no budget: the page was accepted at extract).
pub(crate) fn preview_tokens<'a>(text: &'a str, cfg: Cfg<'_>) -> Option<Vec<Tok<'a>>> {
    html::tokenize_with(text, cfg.template, &mut None).ok()
}

pub(crate) fn skeleton_page(doc: &Document) -> Option<String> {
    let (text, _) = skeleton_text(doc).ok()?;
    if doc.skeleton.parser().is_some_and(|p| p.name == SERVER_PAGES) {
        return None;
    }
    Some(text)
}

pub(crate) fn translated_value(u: &Unit) -> Option<String> {
    target_of(u).map(|t| plain_text(&t))
}

pub(crate) fn quotes_for(text: &str, cfg: Cfg) -> bool {
    cfg.template.is_some() || !browser::raw_quotes_safe(text)
}

/// The template spans of the unit, then the semicolon-less legacy references its source holds (`&nbsp`), which a
/// target writes as they are, not as `&amp;nbsp`.
pub(crate) fn protected_for(cfg: Cfg, u: &html::BodyUnit) -> Vec<String> {
    let core = core_text(u);
    let mut p = protected_of(cfg.template, &core);
    p.extend(bilingua_htmlcore::pyx::legacy_refs(&core));
    p
}
