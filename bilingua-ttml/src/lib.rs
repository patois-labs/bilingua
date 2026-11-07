//! Bilingua TTML / DFXP subtitle filter (`.ttml`, `.dfxp`; IMSC1 text profile documents are TTML). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod inline;
mod lang;
mod scan;
mod textenc;

use bilingua_model::*;
pub use bilingua_model::Limits;

/// The TTML filter. One unit per caption `<p>`; only the content of the `<p>` is text.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ttml;

// Hidden codes (ids start with `~`, never a tag id).
const RAW: &str = "raw"; // the content as written
const META: &str = "meta"; // `<p index>|<preserve 0/1>|<br element name>`

fn code(u: &Unit, key: &str) -> Option<String> {
    u.hidden(key).map(str::to_string)
}

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

impl Ttml {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        guard::check_input(bytes, limits)?;
        guard::check_xml(bytes, limits)?;
        let dec = enc::decode_text(bytes, &enc::DecodeOptions::with_limits(*limits).with_source_lang(opts.source_lang.as_deref()).declared())?;
        let text = &dec.text;
        if enc::is_legacy(&dec.encoding) && enc::declared_encoding(bytes).is_none_or(|e| [encoding_rs::UTF_8, encoding_rs::UTF_16LE, encoding_rs::UTF_16BE].contains(&e)) {
            // XML (and the B6 rule for every XML format): bytes that are not UTF-8 or UTF-16 must name their
            // legacy encoding in the declaration; a missing or a false (UTF-8) declaration is refused, never guessed
            return Err(Error::Encoding("This TTML file is not UTF-8 and does not declare its encoding (<?xml version=\"1.0\" encoding=\"...\"?>). Re-save it as UTF-8, or add the declaration.".into()));
        }
        if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && textenc::encode(&dec.text, &dec.encoding, &MergeOptions::default())? != bytes {
            // double-mapped bytes of the CJK code pages read to the same character and write back as the other
            // spelling; an untouched merge must be the original bytes, so the file is refused, never altered
            return Err(Error::Encoding("This TTML file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into()));
        }
        let sc = scan::scan(text, limits, &std::collections::HashSet::new())?;
        if sc.root_ns == scan::DRAFT_2004 && opts.is_profile(profile::LOCOS) {
            // LocOS knows only the 2006 DFXP drafts and TTML: its profile keeps the refusal
            return Err(scan::perr("not a TTML/DFXP document: the root element must be <tt> in the TTML namespace (http://www.w3.org/ns/ttml)."));
        }
        let mut units: Vec<Unit> = Vec::new();
        let mut parts: Vec<SkelPart> = Vec::new();
        let (mut pos, mut line, mut counted) = (0usize, 1usize, 0usize);
        for (idx, cap) in sc.caps.iter().enumerate() {
            let content = &text[cap.content_start..cap.content_end];
            let (inl, codes) = inline::tokenize(content, cap.preserve);
            if !inline::has_text(&inl) {
                continue;
            }
            guard::check_unit_count(units.len() + 1, limits)?;
            let prefix = cap.qname.rsplit_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
            let mut u = Unit::new(format!("p{}", idx + 1), inl);
            u.codes = codes;
            u.set_hidden(RAW, content);
            u.set_hidden(META, format!("{idx}|{}|{prefix}br", u8::from(cap.preserve)));
            line += text.as_bytes()[counted..cap.content_start].iter().filter(|b| **b == b'\n').count();
            counted = cap.content_start;
            u.context.kind = Some("subtitle".into());
            u.context.style = cap.attrs.style.clone();
            u.context.key = cap.attrs.xml_id.clone();
            u.context.location = Some(format!("line {line}"));
            for (k, v) in [("begin", &cap.attrs.begin), ("end", &cap.attrs.end), ("dur", &cap.attrs.dur), ("region", &cap.attrs.region)] {
                if let Some(v) = v {
                    u.context.extra.insert(k.into(), v.clone());
                }
            }
            if cap.content_start > pos {
                parts.push(SkelPart::Raw(text[pos..cap.content_start].to_string()));
            }
            pos = cap.content_end;
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < text.len() || parts.is_empty() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document::new("ttml", units, Skeleton::text(parts, dec.encoding, parser_info!())).with_source_lang(opts.source_lang.clone().or_else(|| sc.root_lang.clone().filter(|l| !l.trim().is_empty()))).with_target_lang(opts.target_lang.clone()).with_granularity(opts.granularity);
        guard::finish(doc, dec.warnings, limits)
    }
}

impl Filter for Ttml {
    fn id(&self) -> &'static str {
        "ttml"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ttml", "dfxp"]
    }

    /// Tight: XML whose first 4 KB names a TTML or DFXP namespace. Put it before generic XML and `Html`.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let h = head_text(bytes);
        h.trim_start().starts_with('<') && ["www.w3.org/ns/ttml", "www.w3.org/2006/10/ttaf1", "www.w3.org/2006/04/ttaf1", "www.w3.org/2004/11/ttaf1"].iter().any(|n| h.contains(n))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        check_parser(doc, &parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("ttml needs a text skeleton".into()));
        };
        let refs = char_refs(parts, encoding);
        // the original text, and where each unit's content sits in it
        let mut orig = String::new();
        let mut spans: Vec<(usize, usize, usize)> = Vec::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => orig.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    let start = orig.len();
                    orig.push_str(&code(u, RAW).unwrap_or_default());
                    spans.push((start, orig.len(), *i));
                }
            }
        }
        let tag = if opts.should_stamp(doc) { doc.target_lang.as_deref().and_then(lang::canonical_tag) } else { None };
        let rtl = tag.as_deref().is_some_and(lang::is_rtl);
        // the units whose caption content is rewritten, with their translation and caption index
        let mut changed: Vec<(usize, usize, usize, Vec<Inline>, usize)> = Vec::new();
        for (start, end, ui) in &spans {
            let u = &doc.units[*ui];
            if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
                continue;
            }
            let new = u.output();
            if new == u.source() {
                continue; // unchanged text keeps the original bytes (layout whitespace, entity spelling)
            }
            let meta = code(u, META).unwrap_or_default();
            let idx: usize = meta.split('|').next().and_then(|v| v.parse().ok()).ok_or_else(|| Error::Write("unit has no caption index".into()))?;
            changed.push((*start, *end, *ui, new, idx));
        }
        // (start, end, replacement) in file order; ties keep insertion order
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        let mut rebuilt: Vec<usize> = Vec::new(); // caption indexes whose content was rewritten
        let sc = if tag.is_some() {
            // the skeleton passed the limits at extraction (perhaps wider ones than the defaults): only the facts
            // are read again here, so the size budgets are not applied a second time
            let wide = Limits::new().with_max_units(usize::MAX / 64);
            let dir_caps = if rtl { changed.iter().map(|c| c.4).collect() } else { std::collections::HashSet::new() };
            Some(scan::scan(&orig, &wide, &dir_caps).map_err(|e| Error::Write(format!("the skeleton no longer holds a TTML file: {e}")))?)
        } else {
            None
        };
        let root_lang = sc.as_ref().and_then(|s| s.root_lang.clone());
        let src = root_lang.as_deref().map(|l| bilingua_model::py_strip(l).to_lowercase()).unwrap_or_default();
        for (start, end, ui, new, idx) in &changed {
            let (idx, u) = (*idx, &doc.units[*ui]);
            let meta = code(u, META).unwrap_or_default();
            let br = meta.splitn(3, '|').nth(2).unwrap_or("br");
            let original = &orig[*start..*end];
            let lead = &original[..original.len() - bilingua_model::py_lstrip(original).len()];
            let trail = &original[bilingua_model::py_rstrip(original).len()..];
            let mut body = inline::render(new, &u.codes, br);
            if let (Some(t), false) = (&tag, src.is_empty()) {
                body = relang_tags(&body, &src, t);
            }
            edits.push((*start, *end, format!("{lead}{}{trail}", with_char_refs(body, refs))));
            rebuilt.push(idx);
        }
        if let (Some(t), Some(sc)) = (&tag, &sc) {
            let mut done: Vec<(usize, usize)> = edits.iter().map(|(s, e, _)| (*s, *e)).collect();
            done.sort_unstable();
            let (rt_start, rt_end) = sc.root_tag;
            for (vs, ve, val) in &sc.lang_ranges {
                let j = done.partition_point(|(s, _)| s <= vs);
                if j > 0 && *vs < done[j - 1].1 {
                    continue; // inside a rebuilt <p>: already switched
                }
                // <head> metadata is never translated: an explicit source tag there stays
                let in_body = sc.body_start.is_some_and(|b| *vs >= b);
                if (rt_start <= *vs && *vs < rt_end) || (in_body && !src.is_empty() && bilingua_model::py_strip(val).to_lowercase() == src) {
                    edits.push((*vs, *ve, t.clone()));
                }
            }
            if sc.root_lang.is_none() {
                edits.push((sc.root_name_end, sc.root_name_end, format!(" xml:lang=\"{t}\"")));
            }
            if rtl && !rebuilt.is_empty() {
                let new_prefix = ["tts".to_string()].into_iter().chain((1..1000).map(|n| format!("tts{n}"))).find(|p| !sc.prefixes.contains(p)).unwrap_or_else(|| "tts1000".into());
                let mut bind = false;
                for idx in &rebuilt {
                    let cap = sc.caps.get(*idx).ok_or_else(|| Error::Write("caption index out of range".into()))?;
                    if !cap.dir_attrs.is_empty() {
                        for (vs, ve, v) in &cap.dir_attrs {
                            if v != "rtl" {
                                edits.push((*vs, *ve, "rtl".into())); // an explicit ltr (or junk) becomes rtl
                            }
                        }
                    } else {
                        let p = match &cap.style_prefix {
                            Some(p) => p.clone(),
                            None => {
                                bind = true;
                                new_prefix.clone()
                            }
                        };
                        edits.push((cap.name_end, cap.name_end, format!(" {p}:direction=\"rtl\"")));
                    }
                }
                if bind {
                    let ns = scan::STYLING_NS[sc.root_ns][0];
                    edits.push((sc.root_name_end, sc.root_name_end, format!(" xmlns:{new_prefix}=\"{ns}\"")));
                }
            }
        }
        edits.sort_by_key(|e| e.0); // stable
        let mut out = String::with_capacity(orig.len() + 64);
        let mut pos = 0usize;
        for (s, e, rep) in &edits {
            if *s < pos || *e < *s || *e > orig.len() || !orig.is_char_boundary(*s) || !orig.is_char_boundary(*e) {
                return Err(Error::Write("overlapping edits in the TTML skeleton".into()));
            }
            out.push_str(&orig[pos..*s]);
            out.push_str(rep);
            pos = *e;
        }
        out.push_str(&orig[pos..]);
        textenc::encode(&out, encoding, opts)
    }
}

/// Switch `xml:lang` values equal to the source language inside the TAGS of a rebuilt content (never inside text,
/// which is escaped and may quote anything).
fn relang_tags(body: &str, src: &str, lang: &str) -> String {
    if !body.contains("xml:lang") {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len());
    let mut pos = 0;
    for (s, e) in inline::tokens(body) {
        let tok = &body[s..e];
        if tok.starts_with("<!") || tok.starts_with("<?") || tok.starts_with("</") {
            continue;
        }
        out.push_str(&body[pos..s]);
        let mut tp = 0;
        for (vs, ve) in scan::tag_lang_ranges(tok) {
            out.push_str(&tok[tp..vs]);
            if bilingua_model::py_strip(&scan::unescape(&tok[vs..ve])).to_lowercase() == src {
                out.push_str(lang);
            } else {
                out.push_str(&tok[vs..ve]);
            }
            tp = ve;
        }
        out.push_str(&tok[tp..]);
        pos = e;
    }
    out.push_str(&body[pos..]);
    out
}

/// Characters a translated span writes as character references whatever the engine's writer would do.
#[derive(Clone, Copy, PartialEq)]
enum CharRefs {
    None,
    /// UTF-8 bytes under an `encoding="us-ascii"` declaration: everything outside ASCII.
    NonAscii,
    /// GBK / GB2312: the euro sign (the engine writes GBK through GB18030, whose A2 E3 GBK readers cannot read).
    Euro,
}

fn char_refs(parts: &[SkelPart], encoding: &str) -> CharRefs {
    let base = enc::base(encoding);
    if base == "utf-8" {
        if let Some(SkelPart::Raw(head)) = parts.first() {
            if let Some(decl) = head.strip_prefix("<?xml").and_then(|r| r.split("?>").next()) {
                let d = decl.to_ascii_lowercase().replace(' ', "");
                if ["us-ascii", "ascii"].iter().any(|l| d.contains(&format!("encoding=\"{l}\"")) || d.contains(&format!("encoding='{l}'"))) {
                    return CharRefs::NonAscii;
                }
            }
        }
    } else if encoding_rs::Encoding::for_label(base.as_bytes()) == Some(encoding_rs::GBK) {
        return CharRefs::Euro;
    }
    CharRefs::None
}

fn with_char_refs(s: String, refs: CharRefs) -> String {
    let hit = |c: char| match refs {
        CharRefs::None => false,
        CharRefs::NonAscii => !c.is_ascii(),
        CharRefs::Euro => c == '\u{20AC}',
    };
    if !s.chars().any(hit) {
        return s;
    }
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        if hit(c) {
            out.push_str(&format!("&#{};", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}
