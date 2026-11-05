//! Document to HTML: the translated file (`merge`) and the live-preview page (`preview`).

use crate::ent::*;
use crate::extract::{OWNER, RAW, VALUE};
use crate::tok::*;
use bilingua_model::*;
use std::collections::HashMap;

struct Owned {
    part: String,
    rs: usize,
    re: usize,
    unit: usize,
}

struct Ctx<'a> {
    doc: &'a Document,
    enc: Enc,
    preview: bool,
    /// (owner unit id, code id) -> attribute units inside that tag.
    owned: HashMap<(String, String), Vec<Owned>>,
}

fn translated(u: &Unit) -> bool {
    u.translate && u.segments.iter().any(|s| s.has_target())
}

fn label<'a>(u: &'a Unit, key: &str) -> Option<&'a str> {
    u.hidden(key)
}

fn is_rtl(lang: &str) -> bool {
    let p = lang.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    matches!(p.as_str(), "ar" | "he" | "fa" | "ur" | "ps" | "yi" | "dv" | "ug" | "sd")
}

fn is_html_tag(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 6
        && b[..5].eq_ignore_ascii_case(b"<html")
        && matches!(b[5], b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/')
        && s.ends_with('>')
}

fn apply_edits(mut s: String, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (a, b, r) in edits {
        if a <= b && b <= s.len() && s.is_char_boundary(a) && s.is_char_boundary(b) {
            s.replace_range(a..b, &r);
        }
    }
    s
}

/// Sets `lang` and (for right-to-left targets) `dir` on the `<html>` start tag.
fn retarget(tag: &str, lang: &str, enc: Enc) -> String {
    let Some((_, attrs)) = parse_tag_str(tag) else { return tag.to_string() };
    let mut edits = Vec::new();
    let q = |v: &str| format!("\"{}\"", esc_attr(v, enc));
    for a in &attrs {
        if (a.name == "lang" || a.name == "xml:lang") && a.full.is_some() {
            if let Some((x, y)) = a.full {
                edits.push((x, y, q(lang)));
            }
        }
    }
    if is_rtl(lang) {
        match attrs.iter().find(|a| a.name == "dir").and_then(|a| a.full) {
            Some((x, y)) => edits.push((x, y, "\"rtl\"".to_string())),
            None => edits.push((5, 5, " dir=\"rtl\"".to_string())),
        }
    }
    apply_edits(tag.to_string(), edits)
}

impl<'a> Ctx<'a> {
    fn new(doc: &'a Document, preview: bool) -> Result<Ctx<'a>, Error> {
        Ctx::new_with(doc, preview, &MergeOptions::default())
    }

    fn new_with(doc: &'a Document, preview: bool, opts: &MergeOptions) -> Result<Ctx<'a>, Error> {
        let mut enc = match &doc.skeleton {
            Skeleton::Text { encoding, .. } => {
                Enc::from_name(encoding).ok_or_else(|| Error::Write(format!("unknown encoding {encoding}")))?
            }
            _ => return Err(Error::Write("html needs a text skeleton".into())),
        };
        if opts.output_encoding == OutputEncoding::Utf8 && !matches!(enc, Enc::Utf8 | Enc::Utf8Bom) {
            // written as UTF-8: nothing needs a numeric reference any more
            enc = Enc::Utf8;
        }
        let mut owned: HashMap<(String, String), Vec<Owned>> = HashMap::new();
        for (i, u) in doc.units.iter().enumerate() {
            let Some(spec) = u.hidden(OWNER) else { continue };
            let f: Vec<&str> = spec.split('|').collect();
            if f.len() != 5 {
                continue;
            }
            let (Ok(rs), Ok(re)) = (f[3].parse::<usize>(), f[4].parse::<usize>()) else { continue };
            owned.entry((f[0].to_string(), f[1].to_string())).or_default().push(Owned { part: f[2].to_string(), rs, re, unit: i });
        }
        Ok(Ctx { doc, enc, preview, owned })
    }

    fn owned_translated(&self, unit_id: &str) -> bool {
        self.owned
            .iter()
            .filter(|((o, _), _)| o == unit_id)
            .any(|(_, v)| v.iter().any(|x| translated(&self.doc.units[x.unit])))
    }

    fn attr_ids(&self, u: &Unit) -> String {
        u.segments.iter().map(|s| s.id.as_str()).collect::<Vec<_>>().join(",")
    }

    /// One stored tag, with translated attribute values written in (and markers added for the preview).
    fn tag(&self, unit: &Unit, code_id: &str, part: &str, markup: &str) -> String {
        let Some(list) = self.owned.get(&(unit.id.clone(), code_id.to_string())) else {
            return markup.to_string();
        };
        let mut edits = Vec::new();
        let mut ids = Vec::new();
        let mut names = Vec::new();
        for o in list.iter().filter(|o| o.part == part) {
            let au = &self.doc.units[o.unit];
            if translated(au) {
                edits.push((o.rs, o.re, format!("\"{}\"", esc_attr(&plain_text(&au.output()), self.enc))));
            }
            ids.push(self.attr_ids(au));
            names.push(au.context.key.clone().unwrap_or_default());
        }
        let mut out = apply_edits(markup.to_string(), edits);
        if self.preview && !ids.is_empty() {
            let name_end = out[1..].find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>').map(|p| p + 1).unwrap_or(out.len());
            out.insert_str(name_end, &format!(" data-bl-attr-seg=\"{}\" data-bl-attr=\"{}\"", ids.join(" "), names.join(" ")));
        }
        out
    }

    fn inlines(&self, unit: &Unit, v: &[Inline]) -> String {
        let mut out = String::new();
        for i in v {
            match i {
                Inline::Text(t) => out.push_str(&esc_text(t, self.enc)),
                Inline::Tag { id, kind, .. } => {
                    let Some(code) = unit.codes.get(id) else { continue };
                    let (part, markup) = match kind {
                        TagKind::Open => ("open", code.open.as_deref()),
                        TagKind::Close => ("close", code.close.as_deref()),
                        TagKind::Standalone => ("standalone", code.standalone.as_deref()),
                    };
                    if let Some(m) = markup {
                        out.push_str(&self.tag(unit, id, part, m));
                    }
                }
            }
        }
        out
    }

    fn seg_inlines<'u>(&self, seg: &'u Segment) -> &'u [Inline] {
        if !seg.has_target() {
            &seg.source
        } else {
            &seg.target
        }
    }

    fn unit_text(&self, u: &Unit) -> String {
        plain_text(&u.output())
    }

    fn render_merge(&self, u: &Unit) -> String {
        let raw = u.hidden(RAW);
        match label(u, VALUE) {
            Some("attr") => match raw {
                Some(r) if !translated(u) => r.to_string(),
                _ => format!("\"{}\"", esc_attr(&self.unit_text(u), self.enc)),
            },
            Some(_) => match raw {
                Some(r) if !translated(u) => r.to_string(),
                _ => esc_text(&self.unit_text(u), self.enc),
            },
            None => match raw {
                Some(r) if !translated(u) && !self.owned_translated(&u.id) => r.to_string(),
                Some(r) if !u.translate => r.to_string(),
                _ => self.inlines(u, &u.output()),
            },
        }
    }

    fn render_preview(&self, u: &Unit, buf: &mut String) {
        let raw = u.hidden(RAW);
        match label(u, VALUE) {
            Some(kind) => {
                let is_attr = kind == "attr";
                let ids = self.attr_ids(u);
                let name = if is_attr { u.context.key.clone().unwrap_or_default() } else { "#text".to_string() };
                mark_element(buf, &ids, &name);
                match raw {
                    Some(r) if !translated(u) => buf.push_str(r),
                    _ if is_attr => buf.push_str(&format!("\"{}\"", esc_attr(&self.unit_text(u), self.enc))),
                    _ => buf.push_str(&esc_text(&self.unit_text(u), self.enc)),
                }
            }
            None => {
                for (i, seg) in u.segments.iter().enumerate() {
                    if i > 0 {
                        if let Some(sep) = u.separators.get(i - 1) {
                            buf.push_str(&esc_text(sep, self.enc));
                        }
                    }
                    buf.push_str(&format!("<span data-bl-seg=\"{}\">", esc_attr(&seg.id, self.enc)));
                    buf.push_str(&self.inlines(u, self.seg_inlines(seg)));
                    buf.push_str("</span>");
                }
            }
        }
    }
}

/// Adds the attribute-segment markers to the element whose start tag is being written at the end of `buf`.
fn mark_element(buf: &mut String, ids: &str, attr: &str) {
    let Some(ts) = buf.rfind('<') else { return };
    let name_end = buf[ts + 1..]
        .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .map(|p| p + ts + 1)
        .unwrap_or(buf.len());
    let seg_key = "data-bl-attr-seg=\"";
    let attr_key = "data-bl-attr=\"";
    if let (Some(p1), Some(p2)) = (buf[ts..].find(seg_key).map(|p| p + ts), buf[ts..].find(attr_key).map(|p| p + ts)) {
        let q2 = buf[p2 + attr_key.len()..].find('"').map(|q| q + p2 + attr_key.len());
        let q1 = buf[p1 + seg_key.len()..].find('"').map(|q| q + p1 + seg_key.len());
        if let (Some(q1), Some(q2)) = (q1, q2) {
            // The attribute-name list comes later in the tag, so insert there first.
            let (first, second) = if q2 > q1 { ((q2, attr), (q1, ids)) } else { ((q1, ids), (q2, attr)) };
            buf.insert_str(first.0, &format!(" {}", first.1));
            buf.insert_str(second.0, &format!(" {}", second.1));
            return;
        }
    }
    buf.insert_str(name_end, &format!(" data-bl-attr-seg=\"{ids}\" data-bl-attr=\"{attr}\""));
}

fn assemble(doc: &Document, preview: bool, opts: &MergeOptions) -> Result<(String, Enc), Error> {
    let ctx = Ctx::new_with(doc, preview, opts)?;
    let Skeleton::Text { parts, .. } = &doc.skeleton else {
        return Err(Error::Write("html needs a text skeleton".into()));
    };
    let any_translated = doc.units.iter().any(translated);
    let mut html_done = false;
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => {
                if !html_done && is_html_tag(s) {
                    html_done = true;
                    match opts.stamp_lang(doc, any_translated) {
                        Some(l) => out.push_str(&retarget(s, l, ctx.enc)),
                        None => out.push_str(s),
                    }
                } else {
                    out.push_str(s);
                }
            }
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                if preview {
                    ctx.render_preview(u, &mut out);
                } else {
                    out.push_str(&ctx.render_merge(u));
                }
            }
        }
    }
    Ok((out, ctx.enc))
}

pub fn merge_html(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let (s, enc) = assemble(doc, false, opts)?;
    if opts.output_encoding == OutputEncoding::Utf8 {
        // the charset declaration (`<meta charset>`) must say what the bytes are
        return Ok(enc.encode(&bilingua_model::enc::declare_utf8(&s)));
    }
    Ok(enc.encode(&s))
}

const PREVIEW_STYLE: &str = "<style>[data-bl-seg]{outline:none}[data-bl-seg].bl-active{outline:2px solid #3b82f6;outline-offset:2px}</style>";

pub fn preview_html(doc: &Document) -> Option<String> {
    let (s, _) = assemble(doc, true, &MergeOptions::default()).ok()?;
    Some(sanitize_html(&s, true))
}

pub fn render_segment_html(doc: &Document, seg_id: &str) -> Option<String> {
    let ctx = Ctx::new(doc, true).ok()?;
    let (u, seg) = doc.find(seg_id)?;
    match label(u, VALUE) {
        Some("attr") => Some(esc_attr(&ctx.unit_text(u), ctx.enc)),
        Some(_) => Some(esc_text(&ctx.unit_text(u), ctx.enc)),
        None => Some(sanitize_html(&ctx.inlines(u, ctx.seg_inlines(seg)), false)),
    }
}

fn unsafe_url(v: &str) -> bool {
    let d: String = decode_entities(v).chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
    d.starts_with("javascript:") || d.starts_with("vbscript:") || d.starts_with("data:text/html")
}

/// Removes scripts, event handler attributes, script URLs, `<base>` and meta refresh from HTML. With
/// `inject`, also adds the preview `<style>`.
pub fn sanitize_html(html: &str, inject: bool) -> String {
    let toks = tokenize(html);
    let mut insert_at: Option<usize> = None; // token index to insert before
    if inject {
        let mut after_html: Option<usize> = None;
        let mut first_content: Option<usize> = None;
        for (i, t) in toks.iter().enumerate() {
            match t {
                Tok::End { name, .. } if name == "head" && insert_at.is_none() => insert_at = Some(i),
                Tok::Start { name, .. } if name == "html" && after_html.is_none() => after_html = Some(i + 1),
                Tok::Start { .. } | Tok::Text(..) if first_content.is_none() => first_content = Some(i),
                _ => {}
            }
        }
        insert_at = insert_at.or(after_html).or(first_content).or(Some(0));
    }
    let mut out = String::with_capacity(html.len() + 200);
    let mut in_script = false;
    for (i, t) in toks.iter().enumerate() {
        if insert_at == Some(i) {
            out.push_str(PREVIEW_STYLE);
        }
        let (a, b) = t.range();
        match t {
            Tok::Start { name, attrs, .. } => {
                if name == "script" {
                    in_script = true;
                    continue;
                }
                if name == "base" {
                    continue;
                }
                if name == "meta" && attr_value(html, attrs, "http-equiv").is_some_and(|v| v.trim().eq_ignore_ascii_case("refresh")) {
                    continue;
                }
                let mut cuts: Vec<(usize, usize, String)> = Vec::new();
                for at in attrs {
                    let bad_name = (at.name.starts_with("on") && at.name.len() > 2) || at.name == "srcdoc";
                    let bad_url = matches!(at.name.as_str(), "href" | "src" | "action" | "formaction" | "xlink:href" | "data" | "poster")
                        && at.val.is_some_and(|(x, y)| unsafe_url(&html[x..y]));
                    if bad_name || bad_url {
                        let end = at.full.map(|f| f.1).unwrap_or(at.name_end);
                        cuts.push((at.name_start - a, end - a, String::new()));
                    }
                }
                out.push_str(&apply_edits(html[a..b].to_string(), cuts));
            }
            Tok::End { name, .. } if name == "script" => {
                in_script = false;
            }
            Tok::Raw(..) | Tok::Text(..) if in_script => {}
            Tok::End { name, .. } if name == "base" => {}
            _ => {
                if !in_script {
                    out.push_str(&html[a..b]);
                }
            }
        }
    }
    out
}
