#![allow(dead_code)]
//! Helpers shared by the integration tests: the LocOS suite's document builders, token text for units, and a
//! way to translate by paragraph number with LocOS-style text (`⟦r1⟧bold⟦/r1⟧`, `⟦x2⟧`).

use bilingua_icml::Icml;
use bilingua_model::*;

pub const BOLD: &str = "CharacterStyle/Bold";
pub const NORMAL: &str = "CharacterStyle/$ID/[No character style]";

pub const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<?aid style=\"50\" type=\"snippet\" readerVersion=\"6.0\" featureSet=\"513\" product=\"20.0(73)\" ?>\n\
<?aid SnippetType=\"InCopyInterchange\"?>\n\
<Document DOMVersion=\"20.0\" Self=\"d\">\n\
\t<Color Self=\"Color/Black\" Model=\"Process\" Space=\"CMYK\" ColorValue=\"0 0 0 100\" Name=\"Black\"/>\n\
\t<RootCharacterStyleGroup Self=\"rcsg\"><CharacterStyle Self=\"CharacterStyle/Bold\" Name=\"Bold\" FontStyle=\"Bold\"/></RootCharacterStyleGroup>\n";

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_granularity(Granularity::Paragraph)
}

pub fn opts_to(lang: &str) -> ExtractOptions {
    ExtractOptions::new().with_target_lang(lang).with_granularity(Granularity::Paragraph)
}

pub fn psr(inner: &str) -> String {
    psr_style(inner, "ParagraphStyle/$ID/NormalParagraphStyle")
}

pub fn psr_style(inner: &str, style: &str) -> String {
    format!("\t\t<ParagraphStyleRange AppliedParagraphStyle=\"{style}\">\n{inner}\n\t\t</ParagraphStyleRange>")
}

pub fn csr(inner: &str) -> String {
    csr_with(inner, NORMAL, "")
}

pub fn csr_with(inner: &str, style: &str, extra: &str) -> String {
    format!("\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"{style}\"{extra}>{inner}</CharacterStyleRange>")
}

pub fn story(sid: &str, body: &str) -> String {
    format!(
        "\t<Story Self=\"{sid}\" AppliedTOCStyle=\"n\" TrackChanges=\"false\" StoryTitle=\"$ID/\" AppliedNamedGrid=\"n\">\n\
\t\t<StoryPreference OpticalMarginAlignment=\"false\" FrameType=\"TextFrameType\"/>\n\
\t\t<InCopyExportOption IncludeGraphicProxies=\"true\" IncludeAllResources=\"false\"/>\n\
{body}\n\t</Story>\n"
    )
}

pub fn icml(body: &str, resources: &str, tail: &str) -> Vec<u8> {
    format!("{HEAD}{resources}{}{tail}</Document>\n", story("u1", body)).into_bytes()
}

pub fn extract(data: &[u8]) -> Document {
    Icml.extract(data, &opts()).expect("extract")
}

/// A unit's source in LocOS token text: `r` for range pairs, `g` for other pairs, `x` for points.
pub fn tok_text(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let label = u.codes.get(id).and_then(|c| c.label.clone()).unwrap_or_default();
                let letter = if label == "run" { 'r' } else { 'g' };
                match kind {
                    TagKind::Open => s.push_str(&format!("\u{27E6}{letter}{id}\u{27E7}")),
                    TagKind::Close => {
                        // the close takes its open's letter
                        s.push_str(&format!("\u{27E6}/{letter}{id}\u{27E7}"))
                    }
                    TagKind::Standalone => s.push_str(&format!("\u{27E6}x{id}\u{27E7}")),
                }
            }
        }
    }
    s
}

pub fn units(data: &[u8]) -> Vec<String> {
    let d = extract(data);
    d.units.iter().map(|u| tok_text(u, &u.source())).collect()
}

pub fn p_index(u: &Unit) -> usize {
    u.hidden("p").expect("p").parse().expect("number")
}

/// LocOS token text back to inlines (any token, valid or not, becomes a tag with that id).
pub fn parse_tokens(text: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(p) = rest.find('\u{27E6}') {
        if p > 0 {
            out.push(Inline::text(&rest[..p]));
        }
        let Some(end) = rest[p..].find('\u{27E7}') else { break };
        let body = &rest[p + 3..p + end];
        let (kind, body) = match body.strip_prefix('/') {
            Some(b) => (TagKind::Close, b),
            None if body.starts_with('x') => (TagKind::Standalone, body),
            None => (TagKind::Open, body),
        };
        let id: String = body.trim_start_matches(|c: char| c.is_ascii_lowercase()).to_string();
        out.push(Inline::Tag { id, kind, display: String::new(), free: false });
        rest = &rest[p + end + 3..];
    }
    if !rest.is_empty() {
        out.push(Inline::text(rest));
    }
    out
}

pub fn translate_doc(data: &[u8], mapping: &[(usize, &str)], target: Option<&str>) -> Vec<u8> {
    let mut d = Icml.extract(data, &ExtractOptions::new().with_target_lang(target.map(str::to_string)).with_granularity(Granularity::Paragraph)).expect("extract");
    for (p, t) in mapping {
        let u = d.units.iter_mut().find(|u| p_index(u) == *p).unwrap_or_else(|| panic!("no unit for paragraph {p}"));
        u.segments[0].target = parse_tokens(t);
    }
    Icml.merge(&d).expect("merge")
}

pub fn translate(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    translate_doc(data, mapping, None)
}

/// The bytes of a document with one story's element cut out.
pub fn outside(data: &[u8], sid: &str) -> Vec<u8> {
    let key = format!("<Story Self=\"{sid}\"");
    let start = data.windows(key.len()).position(|w| w == key.as_bytes()).expect("story");
    let end_tag = b"</Story>";
    let e = data[start..].windows(end_tag.len()).position(|w| w == end_tag).expect("end") + start + end_tag.len();
    let mut v = data[..start].to_vec();
    v.extend_from_slice(&data[e..]);
    v
}

/// Everything inside `<Content>` elements, joined with `|` (from an independent XML reader).
pub fn visible_text(data: &[u8]) -> String {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    let mut parts = Vec::new();
    loop {
        match r.read_event_into(&mut buf).expect("well-formed output") {
            Event::Start(e) if e.local_name().as_ref() == b"Content" => {
                depth += 1;
                cur.clear();
            }
            Event::End(e) if e.local_name().as_ref() == b"Content" => {
                depth -= 1;
                parts.push(cur.clone());
            }
            Event::Text(t) if depth > 0 => cur.push_str(&t.decode().expect("text")),
            Event::GeneralRef(g) if depth > 0 => {
                let n = g.decode().expect("ref");
                match n.as_ref() {
                    "amp" => cur.push('&'),
                    "lt" => cur.push('<'),
                    "gt" => cur.push('>'),
                    other => {
                        if let Some(h) = other.strip_prefix("#x") {
                            cur.push(char::from_u32(u32::from_str_radix(h, 16).unwrap_or(63)).unwrap_or('?'));
                        } else if let Some(d) = other.strip_prefix('#') {
                            cur.push(char::from_u32(d.parse().unwrap_or(63)).unwrap_or('?'));
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    parts.join("|")
}

/// True when the bytes are well-formed XML for an independent reader (quick-xml).
pub fn well_formed(data: &[u8]) -> bool {
    let mut r = quick_xml::Reader::from_reader(data);
    r.config_mut().check_end_names = true;
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
        buf.clear();
    }
}

/// Number of start/empty elements with this local name.
pub fn count(data: &[u8], local: &str) -> usize {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut n = 0;
    loop {
        match r.read_event_into(&mut buf).expect("well-formed") {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == local.as_bytes() => n += 1,
            Event::Eof => return n,
            _ => {}
        }
        buf.clear();
    }
}

/// Attributes of every element with this local name, in document order.
pub fn attrs_of(data: &[u8], local: &str) -> Vec<Vec<(String, String)>> {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    loop {
        match r.read_event_into(&mut buf).expect("well-formed") {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == local.as_bytes() => {
                let mut v = Vec::new();
                for a in e.attributes().flatten() {
                    v.push((String::from_utf8_lossy(a.key.as_ref()).into_owned(), a.unescape_value().expect("value").into_owned()));
                }
                out.push(v);
            }
            Event::Eof => return out,
            _ => {}
        }
        buf.clear();
    }
}

pub fn attr_vals(data: &[u8], local: &str, attr: &str) -> Vec<Option<String>> {
    attrs_of(data, local).into_iter().map(|v| v.into_iter().find(|(k, _)| k == attr).map(|(_, v)| v)).collect()
}

/// (AppliedCharacterStyle, text) of every outermost CharacterStyleRange inside a Story, text trimmed.
pub fn story_ranges(data: &[u8]) -> Vec<(String, String)> {
    use quick_xml::events::Event;
    let mut r = quick_xml::Reader::from_reader(data);
    let mut buf = Vec::new();
    let (mut depth, mut in_story) = (0usize, false);
    let mut cur = (String::new(), String::new());
    let mut out = Vec::new();
    loop {
        match r.read_event_into(&mut buf).expect("well-formed") {
            Event::Start(e) => {
                let ln = e.local_name();
                if ln.as_ref() == b"Story" {
                    in_story = true;
                } else if in_story && ln.as_ref() == b"CharacterStyleRange" {
                    if depth == 0 {
                        cur = (String::new(), String::new());
                        for a in e.attributes().flatten() {
                            if a.key.as_ref() == b"AppliedCharacterStyle" {
                                cur.0 = a.unescape_value().expect("v").into_owned();
                            }
                        }
                    }
                    depth += 1;
                }
            }
            Event::End(e) => {
                let ln = e.local_name();
                if ln.as_ref() == b"Story" {
                    in_story = false;
                } else if in_story && ln.as_ref() == b"CharacterStyleRange" {
                    depth -= 1;
                    if depth == 0 {
                        out.push((cur.0.clone(), cur.1.trim().to_string()));
                    }
                }
            }
            Event::Text(t) if depth > 0 => cur.1.push_str(&t.decode().expect("t")),
            Event::Eof => return out,
            _ => {}
        }
        buf.clear();
    }
}
