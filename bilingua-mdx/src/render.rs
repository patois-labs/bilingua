//! Document building (extract) and writing (merge) for MDX.

use crate::codespan;
use crate::emph;
use crate::front::{self, Kind};
use crate::mdline;
use crate::pyx::{has_alnum, trim};
use crate::walk::{tag_raw, Item, Span, Walk};
use crate::{Loaded, HIDDEN};
use bilingua_model::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

fn hidden_code(kind: &str, orig: &str, extra: &str) -> CodeData {
    CodeData::new().with_open(kind.to_string()).with_close(extra.to_string()).with_standalone(orig.to_string())
}

fn span_str(s: Span, base: usize) -> String {
    format!("{},{}", s.0 - base, s.1 - base)
}

fn parse_span(s: &str) -> Option<Span> {
    let (a, b) = s.split_once(',')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

pub(crate) fn build_document(w: &Walk, l: &Loaded, opts: &ExtractOptions) -> Result<(Document, Vec<Warning>), Error> {
    let text = w.text;
    let line_no = |off: usize| w.starts.partition_point(|&s| s <= off);
    let mut units: Vec<Unit> = Vec::new();
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    let new_unit = |units: &mut Vec<Unit>, inl: Vec<Inline>| -> usize {
        units.push(Unit::new(format!("u{}", units.len() + 1), inl));
        units.len() - 1
    };
    let mut fm_len = 0;
    if let Some(fm) = &w.fm {
        fm_len = fm.fields.len();
        for f in &fm.fields {
            let ls = w.starts[f.line];
            let (s, e) = (ls + f.start, ls + f.end);
            let i = new_unit(&mut units, vec![Inline::text(f.text.clone())]);
            let kind = if fm.kind == Kind::Yaml { "yaml" } else { "toml" };
            let u = &mut units[i];
            u.codes.insert(HIDDEN.into(), hidden_code("fm", &text[s..e], &format!("{kind}:{}", f.style)));
            u.context.kind = Some("front-matter".into());
            u.context.location = Some(format!("line {}", line_no(s)));
            spans.push((s, e, i));
        }
    }
    let attr_base = fm_len + w.units.len();
    // the attribute units of each line unit, in attribute order (one pass, not one scan per line)
    let mut attrs_of: Vec<Vec<usize>> = vec![Vec::new(); w.units.len()];
    for (k, a) in w.attr_units.iter().enumerate() {
        if let Some(v) = a.line.and_then(|li| attrs_of.get_mut(li)) {
            v.push(k);
        }
    }
    for lu in &w.units {
        let core = &text[lu.start..lu.end];
        if let Some(d) = lu.delim {
            let i = new_unit(&mut units, vec![Inline::text(lu.title.clone())]);
            let u = &mut units[i];
            u.codes.insert(HIDDEN.into(), hidden_code("title", core, &d.to_string()));
            u.context.kind = Some("title".into());
            u.context.location = Some(format!("line {}", line_no(lu.start)));
            spans.push((lu.start, lu.end, i));
            continue;
        }
        let mut inl: Vec<Inline> = Vec::new();
        let mut codes: std::collections::BTreeMap<String, CodeData> = Default::default();
        for it in &lu.items {
            match *it {
                Item::Text(s, e) => {
                    if e > s {
                        inl.push(Inline::text(&text[s..e]));
                    }
                }
                Item::Open(ti) | Item::Close(ti) | Item::Point(ti) => {
                    let tok = &lu.toks[ti];
                    let id = tok.id.to_string();
                    let (kind, raw) = match *it {
                        Item::Open(_) => (TagKind::Open, &text[tok.open.0..tok.open.1]),
                        Item::Close(_) => {
                            let c = tok.close.expect("a closed pair");
                            (TagKind::Close, &text[c.0..c.1])
                        }
                        _ => (TagKind::Standalone, &text[tok.open.0..tok.open.1]),
                    };
                    inl.push(Inline::tag(id.clone(), kind, raw));
                    let label = if text.as_bytes()[tok.open.0] == b'{' { "expression" } else { "jsx" };
                    let e = codes.entry(id.clone()).or_default();
                    e.label = Some(label.into());
                    match kind {
                        TagKind::Open => e.open = Some(raw.to_string()),
                        TagKind::Close => e.close = Some(raw.to_string()),
                        TagKind::Standalone => e.standalone = Some(raw.to_string()),
                    }
                }
            }
        }
        for tok in &lu.toks {
            let mut c = CodeData::new().with_open(span_str(tok.open, lu.start));
            if let Some(cl) = tok.close {
                c.close = Some(span_str(cl, lu.start));
            }
            codes.insert(format!("$t{}", tok.id), c);
        }
        let this = units.len();
        let _ = this;
        let i = new_unit(&mut units, inl);
        let li = i - fm_len;
        for (n, &k) in attrs_of[li].iter().enumerate() {
            let a = &w.attr_units[k];
            codes.insert(
                format!("$a{}", n + 1),
                CodeData::new().with_open((attr_base + k).to_string()).with_standalone(span_str((a.start, a.end), lu.start)),
            );
        }
        let u = &mut units[i];
        u.codes = codes;
        u.codes.insert(HIDDEN.into(), hidden_code("line", core, if lu.block_ok { "1" } else { "0" }));
        u.context.kind = Some(
            if lu.prefix.contains('#') {
                "heading"
            } else if lu.prefix.contains('>') {
                "quote"
            } else if trim(&lu.prefix).is_empty() {
                "paragraph"
            } else {
                "list-item"
            }
            .into(),
        );
        u.context.location = Some(format!("line {}", line_no(lu.start)));
        spans.push((lu.start, lu.end, i));
    }
    for a in &w.attr_units {
        let val = &text[a.start..a.end];
        let i = new_unit(&mut units, vec![Inline::text(val)]);
        let u = &mut units[i];
        u.codes.insert(HIDDEN.into(), hidden_code("attr", val, &(a.quote as char).to_string()));
        u.context.kind = Some("attribute".into());
        u.context.style = Some("a11y:mdx-attr".into());
        u.context.key = Some(a.name.clone());
        u.context.location = Some(format!("line {}", line_no(a.start)));
        if a.line.is_none() {
            spans.push((a.start, a.end, i));
        }
    }
    spans.sort();
    let mut parts: Vec<SkelPart> = Vec::new();
    let mut pos = 0;
    for (s, e, i) in spans {
        if s > pos {
            parts.push(SkelPart::Raw(text[pos..s].to_string()));
        }
        parts.push(SkelPart::Unit(i));
        pos = e;
    }
    if pos < text.len() {
        parts.push(SkelPart::Raw(text[pos..].to_string()));
    }
    let warnings = a11y_warnings(w);
    let doc = Document::new(crate::ID, units, Skeleton::text(parts, l.encoding.clone(), bilingua_model::parser_info!())).with_options(opts);
    Ok((doc, warnings))
}

/// Empty Markdown alt text and `<img>` without alt (the LocOS `a11y_images` report), as warnings.
fn a11y_warnings(w: &Walk) -> Vec<Warning> {
    let text = w.text;
    let mut out = Vec::new();
    for lu in &w.units {
        for (_at, alt, target) in mdline::images(&text[lu.start..lu.end]) {
            if trim(&alt).is_empty() {
                let name = trim(&target).split(' ').next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
                out.push(Warning::new("missing_alt", "empty", format!("Image {name} has empty alt text.")));
            }
        }
    }
    for tag in &w.tags {
        let (closing, _sc, name) = tag_raw(&text[tag.start..tag.end]);
        let lname = name.to_lowercase();
        if closing || (lname != "img" && lname != "image") {
            continue;
        }
        let alt = tag.attrs.iter().find(|a| a.0 == "alt");
        let (kind, ok) = if !tag.names.iter().any(|n| n == "alt") {
            ("missing", false)
        } else if alt.is_some_and(|a| trim(&text[a.1..a.2]).is_empty()) {
            ("empty", false)
        } else {
            ("", true)
        };
        if ok {
            continue;
        }
        let src = tag.attrs.iter().find(|a| a.0 == "src");
        let where_ = src.map_or(name.clone(), |s| trim(&text[s.1..s.2]).rsplit('/').next().unwrap_or("").to_string());
        let msg = if kind == "missing" { format!("Image {where_} has no alt text.") } else { format!("Image {where_} has empty alt text.") };
        out.push(Warning::new("missing_alt", kind, msg));
    }
    out
}

// ---- merge ---------------------------------------------------------------------------------------------------

fn hidden(u: &Unit) -> Option<&CodeData> {
    u.codes.get(HIDDEN)
}

/// The plain text the unit now holds when a target was set, else None.
fn translated(u: &Unit) -> Option<String> {
    if u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    Some(plain_text(&u.output()))
}

/// LocOS text of inlines keyed by tag id: `⟦g1⟧ ⟦/g1⟧ ⟦x2⟧`.
fn locos_ids(v: &[Inline]) -> String {
    locos_ids_at(v).0
}

/// [`locos_ids`], and the byte offsets where the tokens of real codes start. Text that looks like a token (`⟦x2⟧`
/// typed in a source or a translation) is a token match too but starts at no such offset (issue 814).
fn locos_ids_at(v: &[Inline]) -> (String, HashSet<usize>) {
    let mut s = String::new();
    let mut real = HashSet::new();
    for i in v {
        if matches!(i, Inline::Tag { .. }) {
            real.insert(s.len());
        }
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => match kind {
                TagKind::Open => s.push_str(&format!("\u{27E6}g{id}\u{27E7}")),
                TagKind::Close => s.push_str(&format!("\u{27E6}/g{id}\u{27E7}")),
                TagKind::Standalone => s.push_str(&format!("\u{27E6}x{id}\u{27E7}")),
            },
        }
    }
    (s, real)
}

/// For each token match of `s` in order (`ph_re`), whether it is a real code's token.
fn real_flags(s: &str, real: &HashSet<usize>) -> Vec<bool> {
    ph_re().find_iter(s).map(|m| real.contains(&m.start())).collect()
}

fn escape_attr(s: &str, quote: char) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '{' => out.push_str("&#123;"),
            '}' => out.push_str("&#125;"),
            '<' => out.push_str("&#60;"),
            '>' => out.push_str("&#62;"),
            '=' => out.push_str("&#61;"),
            c => out.push(c),
        }
    }
    if quote == '"' {
        out.replace('"', "&quot;")
    } else {
        out.replace('\'', "&#39;")
    }
}

/// A `{ } <` is already escaped only after an odd run of backslashes: after `\\` (an escaped backslash) it would
/// open live JSX or an expression, so it is escaped too. (LocOS looks at one preceding backslash only, so a
/// translation holding `\\{...}` or `\\<X/>` puts code into its build; this is a deliberate difference.)
fn escape_special(seg: &str, out: &mut String) {
    // true right after an odd run of backslashes
    let mut escaped = false;
    for c in seg.chars() {
        if matches!(c, '{' | '}' | '<') && !escaped {
            out.push('\\');
        }
        out.push(c);
        escaped = c == '\\' && !escaped;
    }
}

/// Backslash-escapes `{`, `}` and `<` a translation introduced (outside code spans).
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut pos = 0;
    for (a, b) in codespan::find_all(s, true) {
        escape_special(&s[pos..a], &mut out);
        out.push_str(&s[a..b]);
        pos = b;
    }
    escape_special(&s[pos..], &mut out);
    out
}

struct Attr {
    s: usize,
    e: usize,
    val: String,
}

/// The original text of `core[s..e]` with translated attribute values spliced in.
fn splice(core: &str, attrs: &[Attr], s: usize, e: usize) -> String {
    let mut out = String::new();
    let mut pos = s;
    let from = attrs.partition_point(|a| a.s < s);
    for a in attrs[from..].iter().take_while(|a| a.s < e) {
        out.push_str(&core[pos..a.s]);
        out.push_str(&a.val);
        pos = a.e;
    }
    if pos < e {
        out.push_str(&core[pos..e]);
    }
    out
}

fn ph_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\u{27E6}/?[a-z]?\d+\u{27E7}").expect("static pattern"))
}

fn render_line(doc: &Document, u: &Unit) -> Result<String, Error> {
    let h = hidden(u).ok_or_else(|| Error::Write(format!("unit {} lost its source text", u.id)))?;
    let core = h.standalone.as_deref().unwrap_or_default();
    let block_ok = h.close.as_deref() == Some("1");
    let mut attrs: Vec<Attr> = Vec::new();
    for (k, c) in &u.codes {
        if !k.starts_with("$a") {
            continue;
        }
        let (Some(idx), Some(sp)) = (c.open.as_deref().and_then(|x| x.parse::<usize>().ok()), c.standalone.as_deref().and_then(parse_span)) else { continue };
        let Some(au) = doc.units.get(idx) else { continue };
        let Some(ah) = hidden(au) else { continue };
        let orig = ah.standalone.as_deref().unwrap_or_default();
        let quote = ah.close.as_deref().and_then(|q| q.chars().next()).unwrap_or('"');
        if let Some(new) = translated(au) {
            let val = if new == orig { orig.to_string() } else { escape_attr(&new, quote) };
            attrs.push(Attr { s: sp.0, e: sp.1, val });
        }
    }
    attrs.sort_by_key(|a| a.s);
    // spans come from the document's codes: a stored document that was edited must give an error, never a panic
    let in_core = |s: Span| s.0 <= s.1 && core.get(s.0..s.1).is_some();
    let bad = || Error::Write(format!("unit {} has inline code positions that do not fit its source text", u.id));
    let mut prev_end = 0;
    for a in &attrs {
        if !in_core((a.s, a.e)) || a.s < prev_end {
            return Err(bad());
        }
        prev_end = a.e;
    }
    if u.segments.iter().all(|s| !s.has_target()) {
        return Ok(splice(core, &attrs, 0, core.len()));
    }
    let src = locos_ids(&u.source());
    let (raw, real) = locos_ids_at(&u.output());
    // the emphasis repair reads look-alike text as LocOS does (a token: protected, punctuation); it adds, drops or
    // moves ASCII marks, so while the token matches are unchanged the n-th one after it is the n-th before
    let flags = real_flags(&raw, &real);
    let mut new = raw.clone();
    if new != src {
        new = emph::fold_line_breaks(&src, &new);
        new = emph::fix_cjk_emphasis(&src, &new);
        new = emph::fix_cjk_autolinks(&src, &new, true);
        new = emph::restore_cell_escapes(&src, &new);
        if block_ok {
            new = emph::escape_block_start(&src, &new);
        }
        // a repair that changed the tokens is dropped: the CJK emphasis repair can move a mark into a token
        // (`_⟦x1⟧` to `⟦*x1⟧`), which would write the token as text and lose its code (LocOS does), and the flags
        // would no longer line up
        let before: Vec<&str> = ph_re().find_iter(&raw).map(|m| m.as_str()).collect();
        let after: Vec<&str> = ph_re().find_iter(&new).map(|m| m.as_str()).collect();
        if before != after {
            new = raw.clone();
        }
    }
    let mut toks: HashMap<String, (Span, Option<Span>)> = HashMap::new();
    for (k, c) in &u.codes {
        if let Some(id) = k.strip_prefix("$t") {
            if let Some(o) = c.open.as_deref().and_then(parse_span) {
                let cl = c.close.as_deref().and_then(parse_span);
                if !in_core(o) || cl.is_some_and(|c| !in_core(c)) {
                    return Err(bad());
                }
                toks.insert(id.to_string(), (o, cl));
            }
        }
    }
    let mut out = String::new();
    let mut pos = 0;
    for (m, is_code) in ph_re().find_iter(&new).zip(flags) {
        // a look-alike is text: it stays in the next text run
        if !is_code {
            continue;
        }
        out.push_str(&escape_text(&new[pos..m.start()]));
        pos = m.end();
        let inner = &new[m.start() + 3..m.end() - 3];
        if let Some(rest) = inner.strip_prefix('/') {
            let key = rest.strip_prefix(|c: char| c.is_ascii_lowercase()).unwrap_or(rest);
            if let Some((_, Some(c))) = toks.get(key) {
                out.push_str(&splice(core, &attrs, c.0, c.1));
            }
        } else {
            let key = inner.strip_prefix(|c: char| c.is_ascii_lowercase()).unwrap_or(inner);
            if let Some((o, _)) = toks.get(key) {
                out.push_str(&splice(core, &attrs, o.0, o.1));
            }
        }
    }
    out.push_str(&escape_text(&new[pos..]));
    Ok(out)
}

fn render_unit(doc: &Document, u: &Unit) -> Result<String, Error> {
    let h = hidden(u).ok_or_else(|| Error::Write(format!("unit {} lost its source text", u.id)))?;
    let orig = h.standalone.clone().unwrap_or_default();
    let extra = h.close.clone().unwrap_or_default();
    match h.open.as_deref() {
        Some("line") => render_line(doc, u),
        Some("fm") => {
            let Some(new) = translated(u) else { return Ok(orig) };
            if new == plain_text(&u.source()) {
                return Ok(orig);
            }
            let (k, style) = extra.split_once(':').unwrap_or(("yaml", "plain"));
            let kind = if k == "toml" { Kind::Toml } else { Kind::Yaml };
            Ok(front::encode_value(kind, style, &new).unwrap_or(orig))
        }
        Some("title") => {
            let Some(new) = translated(u) else { return Ok(orig) };
            if new == plain_text(&u.source()) {
                return Ok(orig);
            }
            Ok(mdline::encode_link_title(&new, extra.chars().next().unwrap_or('"')))
        }
        Some("attr") => {
            let Some(new) = translated(u) else { return Ok(orig) };
            if new == orig {
                return Ok(orig);
            }
            Ok(escape_attr(&new, extra.chars().next().unwrap_or('"')))
        }
        _ => Err(Error::Write(format!("unit {} has an unknown kind", u.id))),
    }
}

pub(crate) fn merge(doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
    // no language stamp in MDX
    check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("mdx needs a text skeleton".into()));
    };
    let mut out = String::new();
    for p in parts {
        match p {
            SkelPart::Raw(s) => out.push_str(s),
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                out.push_str(&render_unit(doc, u)?);
            }
        }
    }
    enc::encode_text_with(&out, encoding, opts)
}

#[allow(dead_code)]
fn _keep(_: &str) -> bool {
    has_alnum("")
}
