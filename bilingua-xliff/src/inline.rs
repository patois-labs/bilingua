//! Reading and writing the inline content of a `source` / `target` / `mrk` element:
//! text plus tags. Also remembers the original markup of every tag so a rewritten
//! target keeps attributes the UI never sees (`ctype`, `equivStart`, and so on).

use crate::attrs::{decode_entity, escape_attr, escape_text, TagInfo};
use crate::error::Error;
use bilingua_model::{Inline, TagKind};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

/// Which family of inline elements to expect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// XLIFF 2.x: `ph`, `pc`, `sc`, `ec`, `sm`, `em`, `mrk`, `cp`.
    X2,
    /// XLIFF 1.2 family (also SDLXLIFF and MQXLIFF): `g`, `x`, `bpt`, `ept`, `ph`, `it`, `bx`, `ex`, `mrk`.
    X12,
}

/// The original markup of one tag.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RawTag {
    pub id: String,
    pub kind: TagKind,
    pub raw: String,
}

/// Display strings for SDLXLIFF tag ids, taken from `<tag-defs>`.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TagDef {
    pub open: Option<String>,
    pub close: Option<String>,
    pub standalone: Option<String>,
}

pub type TagDefs = HashMap<String, TagDef>;

#[derive(Debug, Default)]
pub struct Parsed {
    pub inlines: Vec<Inline>,
    pub raws: Vec<RawTag>,
}

fn xml_err(pos: usize, msg: impl ToString) -> Error {
    Error::Xml { pos, msg: msg.to_string() }
}

/// Fast path: plain text with no markup and no entities.
fn plain_text(buf: &[u8]) -> Option<Inline> {
    if buf.iter().any(|&b| b == b'<' || b == b'&') {
        return None;
    }
    std::str::from_utf8(buf).ok().map(|s| Inline::Text(s.to_owned()))
}

struct Builder<'a> {
    buf: &'a [u8],
    flavor: Flavor,
    defs: Option<&'a TagDefs>,
    /// XLIFF 2 `<originalData>` of the unit: data id to text, for `dataRef`.
    data: Option<&'a HashMap<String, String>>,
    out: Parsed,
    text: String,
    counter: usize,
}

impl Builder<'_> {
    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            self.out.inlines.push(Inline::Text(std::mem::take(&mut self.text)));
        }
    }

    fn display(&self, id: &str, kind: TagKind, tag: &TagInfo) -> String {
        if let Some(def) = self.defs.and_then(|d| d.get(id)) {
            let hit = match kind {
                TagKind::Open => def.open.as_ref(),
                TagKind::Close => def.close.as_ref(),
                TagKind::Standalone => def.standalone.as_ref(),
            };
            if let Some(s) = hit {
                return s.clone();
            }
        }
        let buf = self.buf;
        let pick = |names: &[&str]| names.iter().find_map(|n| tag.value(buf, n)).filter(|s| !s.is_empty());
        let shown = match kind {
            TagKind::Open => pick(&["dispStart", "disp"]),
            TagKind::Close => pick(&["dispEnd", "disp"]),
            TagKind::Standalone => pick(&["disp", "equiv", "equiv-text"]),
        };
        shown.unwrap_or_else(|| match kind {
            TagKind::Open => format!("<{id}>"),
            TagKind::Close => format!("</{id}>"),
            TagKind::Standalone => format!("<{id}/>"),
        })
    }

    fn push_tag(&mut self, id: String, kind: TagKind, raw: &[u8], tag: &TagInfo) {
        self.flush_text();
        let display = self.display(&id, kind, tag);
        let free = kind == TagKind::Standalone && self.is_free(raw, tag);
        self.out.raws.push(RawTag { id: id.clone(), kind, raw: String::from_utf8_lossy(raw).into_owned() });
        self.out.inlines.push(Inline::tag(id, kind, display).with_free(free));
    }

    /// The Localization OS rule for an `x` or `ph` placeholder that a translation may move: it needs an `id`,
    /// must not stand for one side of a formatting pair (`START_`/`CLOSE_` ids, an `equiv` of that shape, a
    /// bold/italic/underline/link `ctype`) and the code it stands for must not be markup (`<b>`, `</i>`) or an
    /// unnumbered argument (`%s`, `%d`, `{}`).
    fn is_free(&self, raw: &[u8], tag: &TagInfo) -> bool {
        let buf = self.buf;
        let name = tag.name_str(buf).rsplit(':').next().unwrap_or("");
        if !matches!(name, "x" | "ph") {
            return false;
        }
        let Some(id) = tag.value(buf, "id") else { return false };
        let pair_side = |s: &str| s.starts_with("START_") || s.starts_with("CLOSE_");
        if pair_side(&id) || tag.value(buf, "equiv").is_some_and(|e| pair_side(&e)) {
            return false;
        }
        if let Some(c) = tag.value(buf, "ctype") {
            if matches!(c.to_ascii_lowercase().as_str(), "bold" | "italic" | "underline" | "link") {
                return false;
            }
        }
        let mut codes: Vec<String> = ["equiv-text", "equiv", "disp"].iter().filter_map(|n| tag.value(buf, n)).collect();
        // The text of a `ph` written with content, up to its first child element.
        if let (Some(a), Some(b)) = (raw.iter().position(|&c| c == b'>'), raw.iter().rposition(|&c| c == b'<')) {
            let self_closed = a > 0 && raw[a - 1] == b'/';
            if a < b && !self_closed {
                let inner = &raw[a + 1..b];
                let first = inner.iter().position(|&c| c == b'<').unwrap_or(inner.len());
                codes.push(crate::attrs::unescape_attr(&inner[..first]));
            }
        }
        if let (Some(r), Some(table)) = (tag.value(buf, "dataRef"), self.data) {
            if let Some(t) = table.get(&r) {
                codes.push(t.clone());
            }
        }
        for code in codes.iter().map(|c| c.trim()).filter(|c| !c.is_empty()) {
            let side = code.starts_with('<') && !code.ends_with("/>");
            if side || order_bound(code) {
                return false;
            }
        }
        true
    }

    fn fresh_id(&mut self, name: &str) -> String {
        self.counter += 1;
        format!("_{}{}", name, self.counter)
    }

    /// Id of an element, using the attribute names that pair it with its partner.
    fn id_of(&mut self, tag: &TagInfo, name: &str, keys: &[&str]) -> String {
        for k in keys {
            if let Some(v) = tag.value(self.buf, k) {
                return v;
            }
        }
        self.fresh_id(name)
    }
}

/// What an element does inline.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Has an end tag and nests (`g`, `pc`, `mrk`).
    Wrapper,
    /// Counts as one tag even if written with content (`ph`, `bpt`, `ept`, `it`, `sc`, `ec`).
    Atom(TagKind),
    Unknown,
}

fn role(flavor: Flavor, name: &str, empty: bool) -> Role {
    match (flavor, name) {
        (_, "mrk") => Role::Wrapper,
        (Flavor::X2, "pc") | (Flavor::X12, "g") => Role::Wrapper,
        (Flavor::X2, "sc") | (Flavor::X2, "sm") => Role::Atom(TagKind::Open),
        (Flavor::X2, "ec") | (Flavor::X2, "em") => Role::Atom(TagKind::Close),
        (Flavor::X2, "ph") | (Flavor::X2, "cp") => Role::Atom(TagKind::Standalone),
        (Flavor::X12, "bpt") | (Flavor::X12, "bx") => Role::Atom(TagKind::Open),
        (Flavor::X12, "ept") | (Flavor::X12, "ex") => Role::Atom(TagKind::Close),
        (Flavor::X12, "ph") | (Flavor::X12, "it") | (Flavor::X12, "x") => Role::Atom(TagKind::Standalone),
        _ => {
            let _ = empty;
            Role::Unknown
        }
    }
}

/// Parse the content of a `source` / `target` / segment `mrk` element.
pub fn parse_inlines(
    buf: &[u8],
    flavor: Flavor,
    defs: Option<&TagDefs>,
    data: Option<&HashMap<String, String>>,
) -> Result<Parsed, Error> {
    if let Some(t) = plain_text(buf) {
        let inlines = match &t {
            Inline::Text(s) if s.is_empty() => Vec::new(),
            _ => vec![t],
        };
        return Ok(Parsed { inlines, raws: Vec::new() });
    }
    let mut b = Builder { buf, flavor, defs, data, out: Parsed::default(), text: String::new(), counter: 0 };
    let mut rd = Reader::from_reader(buf);
    // Open wrappers: their id.
    let mut stack: Vec<(String, Option<String>)> = Vec::new();
    // Inside an atom written with content: (start offset, depth, kind, id, tag).
    let mut atom: Option<(usize, usize, TagKind, String, TagInfo)> = None;
    loop {
        let start = rd.buffer_position() as usize;
        let ev = rd.read_event().map_err(|e| xml_err(start, e))?;
        let end = rd.buffer_position() as usize;
        match ev {
            Event::Eof => break,
            Event::Text(t) => {
                if atom.is_none() {
                    let s = t.decode().map_err(|e| xml_err(start, e))?;
                    b.text.push_str(&s);
                }
            }
            Event::CData(t) => {
                if atom.is_none() {
                    let s = t.decode().map_err(|e| xml_err(start, e))?;
                    b.text.push_str(&s);
                }
            }
            Event::GeneralRef(r) => {
                if atom.is_none() {
                    if let Ok(Some(c)) = r.resolve_char_ref() {
                        b.text.push(c);
                    } else {
                        let name = r.decode().map_err(|e| xml_err(start, e))?;
                        match decode_entity(&name) {
                            Some(c) => b.text.push(c),
                            None => {
                                b.text.push('&');
                                b.text.push_str(&name);
                                b.text.push(';');
                            }
                        }
                    }
                }
            }
            Event::Start(_) => {
                if let Some(a) = atom.as_mut() {
                    a.1 += 1;
                    continue;
                }
                let tag = TagInfo::parse(buf, start, end);
                let name = tag.name_str(buf).rsplit(':').next().unwrap_or("").to_owned();
                match role(flavor, &name, false) {
                    Role::Wrapper => {
                        let id = b.id_of(&tag, &name, &["id", "mid", "sdl:cid"]);
                        b.push_tag(id.clone(), TagKind::Open, &buf[start..end], &tag);
                        stack.push((id, tag.value(buf, "dispEnd").filter(|s| !s.is_empty())));
                    }
                    Role::Atom(kind) => {
                        let id = atom_id(&mut b, &tag, &name, kind);
                        atom = Some((start, 1, kind, id, tag));
                    }
                    Role::Unknown => {
                        // Unknown wrapper: keep it as an atomic pair so nothing is lost.
                        let id = b.fresh_id(&name);
                        b.push_tag(id.clone(), TagKind::Open, &buf[start..end], &tag);
                        stack.push((id, None));
                    }
                }
            }
            Event::End(_) => {
                if let Some(a) = atom.as_mut() {
                    a.1 -= 1;
                    if a.1 == 0 {
                        let (s, _, kind, id, tag) = atom.take().unwrap_or_else(|| unreachable!());
                        b.push_tag(id, kind, &buf[s..end], &tag);
                    }
                    continue;
                }
                let (id, close_disp) = stack.pop().ok_or_else(|| xml_err(start, "unbalanced end tag"))?;
                let tag_end = TagInfo { name: start..end, attrs: Vec::new(), insert_pos: end };
                b.push_tag(id, TagKind::Close, &buf[start..end], &tag_end);
                if let (Some(d), Some(Inline::Tag { display, .. })) = (close_disp, b.out.inlines.last_mut()) {
                    *display = d;
                }
            }
            Event::Empty(_) => {
                if atom.is_some() {
                    continue;
                }
                let tag = TagInfo::parse(buf, start, end);
                let name = tag.name_str(buf).rsplit(':').next().unwrap_or("").to_owned();
                let (kind, id) = match role(flavor, &name, true) {
                    Role::Atom(kind) => (kind, atom_id(&mut b, &tag, &name, kind)),
                    Role::Wrapper | Role::Unknown => {
                        let id = b.fresh_id(&name);
                        (TagKind::Standalone, id)
                    }
                };
                b.push_tag(id, kind, &buf[start..end], &tag);
            }
            Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_) => {}
        }
    }
    b.flush_text();
    Ok(b.out)
}

/// Phrase writes inline codes as plain text: `{1}` (standalone), `{1>` (open) and `<1}` (close; the
/// `<` is escaped as `&lt;` in the file). Turn them into tags. Each tag keeps its text as the raw
/// markup, so a rewritten target writes the same bytes the source has. Text with a lone `{` or a
/// non-numeric body is left alone. The ids (`{1}`, `{1>`) cannot clash with the numeric ids of `<x/>`.
pub fn split_brace_codes(p: Parsed) -> Parsed {
    let mut out = Parsed { inlines: Vec::with_capacity(p.inlines.len()), raws: p.raws };
    for item in p.inlines {
        let Inline::Text(s) = item else {
            out.inlines.push(item);
            continue;
        };
        let b = s.as_bytes();
        let mut plain_from = 0usize;
        let mut i = 0usize;
        while i < b.len() {
            let found = match b[i] {
                b'{' | b'<' => brace_code_at(b, i),
                _ => None,
            };
            let Some((len, kind)) = found else {
                i += 1;
                continue;
            };
            if plain_from < i {
                out.inlines.push(Inline::Text(s[plain_from..i].to_owned()));
            }
            let shown = &s[i..i + len];
            let digits = shown.trim_matches(|c| c == '{' || c == '}' || c == '<' || c == '>');
            let (id, raw) = match kind {
                TagKind::Standalone => (format!("{{{digits}}}"), shown.to_owned()),
                TagKind::Open => (format!("{{{digits}>"), format!("{{{digits}&gt;")),
                TagKind::Close => (format!("{{{digits}>"), format!("&lt;{digits}}}")),
            };
            if !out.raws.iter().any(|r| r.id == id && r.kind == kind) {
                out.raws.push(RawTag { id: id.clone(), kind, raw });
            }
            out.inlines.push(Inline::tag(id, kind, shown.to_owned()));
            i += len;
            plain_from = i;
        }
        if plain_from < b.len() {
            out.inlines.push(Inline::Text(s[plain_from..].to_owned()));
        }
    }
    out
}

/// `{N}`, `{N>` or `<N}` at `i`: (byte length, kind).
fn brace_code_at(b: &[u8], i: usize) -> Option<(usize, TagKind)> {
    let mut j = i + 1;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    if j == i + 1 || j - i > 10 || j >= b.len() {
        return None;
    }
    match (b[i], b[j]) {
        (b'{', b'}') => Some((j + 1 - i, TagKind::Standalone)),
        (b'{', b'>') => Some((j + 1 - i, TagKind::Open)),
        (b'<', b'}') => Some((j + 1 - i, TagKind::Close)),
        _ => None,
    }
}

fn atom_id(b: &mut Builder<'_>, tag: &TagInfo, name: &str, kind: TagKind) -> String {
    match (b.flavor, kind) {
        // XLIFF 2: `ec` and `em` point at their start with `startRef`.
        (Flavor::X2, TagKind::Close) => b.id_of(tag, name, &["startRef", "id"]),
        (Flavor::X2, _) => {
            if name == "cp" {
                let hex = tag.value(b.buf, "hex").unwrap_or_default();
                b.counter += 1;
                format!("cp{}.{}", hex, b.counter)
            } else {
                b.id_of(tag, name, &["id"])
            }
        }
        // XLIFF 1.2: `rid` pairs `bpt` with `ept`; otherwise `id` does.
        (Flavor::X12, TagKind::Standalone) => b.id_of(tag, name, &["id"]),
        (Flavor::X12, _) => b.id_of(tag, name, &["rid", "id"]),
    }
}

/// Write inlines as markup. Tags use their original markup when known.
pub fn serialize(inlines: &[Inline], flavor: Flavor, raws: &[RawTag]) -> String {
    let mut out = String::new();
    for i in inlines {
        match i {
            Inline::Text(t) => escape_text(t, &mut out),
            Inline::Tag { id, kind, .. } => {
                if let Some(r) = raws.iter().find(|r| &r.id == id && r.kind == *kind) {
                    out.push_str(&r.raw);
                } else {
                    generate_tag(id, *kind, flavor, &mut out);
                }
            }
        }
    }
    out
}

fn generate_tag(id: &str, kind: TagKind, flavor: Flavor, out: &mut String) {
    let id = escape_attr(id);
    match (flavor, kind) {
        (Flavor::X2, TagKind::Open) => out.push_str(&format!("<pc id=\"{id}\">")),
        (Flavor::X2, TagKind::Close) => out.push_str("</pc>"),
        (Flavor::X2, TagKind::Standalone) => out.push_str(&format!("<ph id=\"{id}\"/>")),
        (Flavor::X12, TagKind::Open) => out.push_str(&format!("<g id=\"{id}\">")),
        (Flavor::X12, TagKind::Close) => out.push_str("</g>"),
        (Flavor::X12, TagKind::Standalone) => out.push_str(&format!("<x id=\"{id}\"/>")),
    }
}

/// True when the whole code is one unnumbered printf argument (`%s`, `%d`, `%5.2f`, `%ld`, `%@`) or `{}`:
/// moving it in a translation would swap which value it shows.
fn order_bound(code: &str) -> bool {
    if code == "{}" {
        return true;
    }
    let Some(rest) = code.strip_prefix('%') else { return false };
    // `%1$s` names its argument, so it may move.
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && rest.as_bytes().get(digits) == Some(&b'$') {
        return false;
    }
    let rest = rest.trim_start_matches(['-', '+', ' ', '#', '0', '\'']);
    let rest = match rest.strip_prefix('*') {
        Some(r) => r,
        None => rest.trim_start_matches(|c: char| c.is_ascii_digit()),
    };
    let rest = match rest.strip_prefix('.') {
        Some(r) => match r.strip_prefix('*') {
            Some(r2) => r2,
            None if r.starts_with(|c: char| c.is_ascii_digit()) => r.trim_start_matches(|c: char| c.is_ascii_digit()),
            None => return false,
        },
        None => rest,
    };
    let conv = |r: &str| r.len() == 1 && "@diouxXcCsSpaAeEfFgG".contains(r);
    ["hh", "h", "ll", "l", "q", "L", "z", "t", "j", "I64", "I32", "I", ""].iter().any(|m| rest.strip_prefix(m).is_some_and(conv))
}

/// Plain text of inlines (tags dropped), for search.
pub fn plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for i in inlines {
        if let Inline::Text(t) = i {
            s.push_str(t);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(p: &Parsed) -> Vec<String> {
        p.inlines
            .iter()
            .map(|i| match i {
                Inline::Text(t) => format!("T:{t}"),
                Inline::Tag { id, kind, .. } => format!("{kind:?}:{id}"),
            })
            .collect()
    }

    #[test]
    fn entities_and_char_refs_become_text() {
        let p = parse_inlines(b"a &amp; b &#xA0;c &lt;d&gt; &#8364;", Flavor::X12, None, None).unwrap();
        assert_eq!(kinds(&p), ["T:a & b \u{a0}c <d> \u{20ac}"]);
    }

    #[test]
    fn unknown_entity_is_kept_as_written() {
        let p = parse_inlines(b"x &nbsp; y<x id=\"1\"/>", Flavor::X12, None, None).unwrap();
        assert_eq!(kinds(&p), ["T:x &nbsp; y", "Standalone:1"]);
    }

    #[test]
    fn xliff12_paired_and_isolated_tags() {
        let p = parse_inlines(
            b"<bpt id=\"1\" rid=\"7\">{b}</bpt>hi<ept id=\"2\" rid=\"7\">{/b}</ept><ph id=\"3\">&lt;br/&gt;</ph><it id=\"4\" pos=\"open\">x</it>",
            Flavor::X12,
            None,
            None,
        )
        .unwrap();
        assert_eq!(kinds(&p), ["Open:7", "T:hi", "Close:7", "Standalone:3", "Standalone:4"]);
        assert_eq!(p.raws[2].raw, "<ph id=\"3\">&lt;br/&gt;</ph>");
    }

    #[test]
    fn xliff2_start_end_markers_and_cdata() {
        let p = parse_inlines(b"<sc id=\"s1\"/>a<![CDATA[<raw>]]><ec startRef=\"s1\"/><ph id=\"p\" disp=\"{x}\"/>", Flavor::X2, None, None).unwrap();
        assert_eq!(kinds(&p), ["Open:s1", "T:a<raw>", "Close:s1", "Standalone:p"]);
        assert!(matches!(&p.inlines[3], Inline::Tag { display, .. } if display == "{x}"));
    }

    #[test]
    fn serialize_generates_tags_that_have_no_original_markup() {
        let v = vec![
            Inline::tag("5", TagKind::Open, ""),
            Inline::text("a&b"),
            Inline::tag("5", TagKind::Close, ""),
            Inline::tag("6", TagKind::Standalone, ""),
        ];
        assert_eq!(serialize(&v, Flavor::X12, &[]), "<g id=\"5\">a&amp;b</g><x id=\"6\"/>");
        assert_eq!(serialize(&v, Flavor::X2, &[]), "<pc id=\"5\">a&amp;b</pc><ph id=\"6\"/>");
    }

    #[test]
    fn empty_and_plain_content_take_the_fast_path() {
        assert!(parse_inlines(b"", Flavor::X2, None, None).unwrap().inlines.is_empty());
        let p = parse_inlines("\u{3053}\u{3093}".as_bytes(), Flavor::X2, None, None).unwrap();
        assert_eq!(kinds(&p), ["T:\u{3053}\u{3093}"]);
    }
}
