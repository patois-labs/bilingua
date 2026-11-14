//! Shared helpers: a hand-built IDML package (no InDesign needed), LocOS token text for units, and tree helpers.
#![allow(dead_code)]

use bilingua_idml::xml::{Id, ParseLimits, Tree};
use bilingua_idml::Idml;
use bilingua_model::*;
use bilingua_zip::{EntryMeta, ZipLimits};

pub const MIMETYPE: &str = "application/vnd.adobe.indesign-idml-package";
pub const PKG_NS: &str = "http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging";
pub const DATE: [u16; 6] = [2026, 10, 10, 9, 30, 0];
pub const BOLD: &str = "CharacterStyle/Bold";
pub const NOSTYLE: &str = "CharacterStyle/$ID/[No character style]";

pub fn story_xml(sid: &str, body: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<idPkg:Story xmlns:idPkg=\"{PKG_NS}\" DOMVersion=\"20.0\">\n\
\t<Story Self=\"{sid}\" AppliedTOCStyle=\"n\" TrackChanges=\"false\">\n\
\t\t<StoryPreference OpticalMarginAlignment=\"false\"/>\n\
{body}\n\
\t</Story>\n\
</idPkg:Story>\n"
    )
    .into_bytes()
}

pub fn psr(inner: &str) -> String {
    psr_s(inner, "ParagraphStyle/$ID/NormalParagraphStyle")
}
pub fn psr_s(inner: &str, style: &str) -> String {
    format!("\t\t<ParagraphStyleRange AppliedParagraphStyle=\"{style}\">\n{inner}\n\t\t</ParagraphStyleRange>")
}
pub fn csr(inner: &str) -> String {
    csr_x(inner, NOSTYLE, "")
}
pub fn csr_b(inner: &str, style: &str) -> String {
    csr_x(inner, style, "")
}
pub fn csr_x(inner: &str, style: &str, extra: &str) -> String {
    format!("\t\t\t<CharacterStyleRange AppliedCharacterStyle=\"{style}\"{extra}>{inner}</CharacterStyleRange>")
}

pub fn designmap(stories: &[&str], conditions: &str, layers: &str) -> Vec<u8> {
    let refs: String = stories.iter().map(|s| format!("\t<idPkg:Story src=\"Stories/Story_{s}.xml\"/>\n")).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<?aid style=\"50\" type=\"document\" readerVersion=\"6.0\" featureSet=\"257\" product=\"20.0(73)\" ?>\n\
<Document xmlns:idPkg=\"{PKG_NS}\" DOMVersion=\"20.0\" Self=\"d\">\n\
{layers}{conditions}\
\t<idPkg:Graphic src=\"Resources/Graphic.xml\"/>\n\
\t<idPkg:Spread src=\"Spreads/Spread_s1.xml\"/>\n\
{refs}</Document>\n"
    )
    .into_bytes()
}

pub fn spread(frames: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<idPkg:Spread xmlns:idPkg=\"{PKG_NS}\" DOMVersion=\"20.0\"><Spread Self=\"s1\">{frames}</Spread></idPkg:Spread>\n"
    )
    .into_bytes()
}

#[derive(Default)]
pub struct Pkg {
    pub stories: Vec<(String, String)>,
    pub order: Option<Vec<String>>,
    pub conditions: String,
    pub layers: String,
    pub frames: String,
    pub extra: Vec<(String, Vec<u8>)>,
}

impl Pkg {
    pub fn new(stories: &[(&str, String)]) -> Pkg {
        Pkg { stories: stories.iter().map(|(a, b)| (a.to_string(), b.clone())).collect(), ..Default::default() }
    }
    pub fn one(body: String) -> Pkg {
        Pkg::new(&[("a", body)])
    }
    pub fn order(mut self, o: &[&str]) -> Pkg {
        self.order = Some(o.iter().map(|s| s.to_string()).collect());
        self
    }
    pub fn conditions(mut self, c: &str) -> Pkg {
        self.conditions = c.into();
        self
    }
    pub fn layers(mut self, c: &str) -> Pkg {
        self.layers = c.into();
        self
    }
    pub fn frames(mut self, c: &str) -> Pkg {
        self.frames = c.into();
        self
    }
    pub fn members(&self) -> Vec<(String, Vec<u8>)> {
        let order: Vec<String> = self.order.clone().unwrap_or_else(|| self.stories.iter().map(|s| s.0.clone()).collect());
        let ord: Vec<&str> = order.iter().map(|s| s.as_str()).collect();
        let mut m: Vec<(String, Vec<u8>)> = vec![
            ("mimetype".into(), MIMETYPE.as_bytes().to_vec()),
            ("designmap.xml".into(), designmap(&ord, &self.conditions, &self.layers)),
            ("META-INF/container.xml".into(), b"<?xml version=\"1.0\"?><container/>".to_vec()),
            ("Resources/Graphic.xml".into(), b"<?xml version=\"1.0\"?><Graphic>\n\t<Color Self=\"c\"/>\n</Graphic>".to_vec()),
            ("Spreads/Spread_s1.xml".into(), spread(&self.frames)),
        ];
        for (sid, body) in &self.stories {
            m.push((format!("Stories/Story_{sid}.xml"), story_xml(sid, body)));
        }
        m.extend(self.extra.clone());
        m
    }
    pub fn build(&self) -> Vec<u8> {
        zip_of(&self.members())
    }
}

pub fn zip_of(members: &[(String, Vec<u8>)]) -> Vec<u8> {
    let entries: Vec<(EntryMeta, Vec<u8>)> = members
        .iter()
        .map(|(n, d)| (EntryMeta { name: n.clone(), stored: n == "mimetype", dir: false, modified: Some(DATE) }, d.clone()))
        .collect();
    bilingua_zip::write_zip(&entries).unwrap()
}

pub fn package(body: String) -> Vec<u8> {
    Pkg::one(body).build()
}

pub fn members_of(data: &[u8]) -> Vec<bilingua_zip::RawEntry> {
    bilingua_zip::read_zip("idml", data, &ZipLimits::default()).unwrap()
}

pub fn extract(data: &[u8]) -> Document {
    Idml.extract(data, &ExtractOptions::default()).unwrap()
}
pub fn extract_for(data: &[u8], lang: &str) -> Document {
    Idml.extract(data, &ExtractOptions::new().with_target_lang(lang)).unwrap()
}

/// A unit as LocOS shows it: `⟦r1⟧bold⟦/r1⟧`.
pub fn tok(u: &Unit) -> String {
    inline_tok(u, &u.segments[0].source)
}

pub fn inline_tok(u: &Unit, src: &[Inline]) -> String {
    let mut s = String::new();
    for i in src {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let label = u.codes.get(id).and_then(|c| c.label.clone()).unwrap_or_default();
                let l = match kind {
                    TagKind::Standalone => 'x',
                    _ if label == "run" => 'r',
                    _ => 'g',
                };
                match kind {
                    TagKind::Close => s.push_str(&format!("\u{27E6}/{l}{id}\u{27E7}")),
                    _ => s.push_str(&format!("\u{27E6}{l}{id}\u{27E7}")),
                }
            }
        }
    }
    s
}

pub fn texts(doc: &Document) -> Vec<String> {
    doc.units.iter().map(tok).collect()
}

/// LocOS token text back to inlines (any id, valid or not).
pub fn from_tok(text: &str) -> Vec<Inline> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(p) = rest.find('\u{27E6}') {
        if p > 0 {
            out.push(Inline::text(&rest[..p]));
        }
        let after = &rest[p + 3..];
        let Some(e) = after.find('\u{27E7}') else {
            out.push(Inline::text(&rest[p..]));
            rest = "";
            break;
        };
        let inner = &after[..e];
        rest = &after[e + 3..];
        let (close, inner) = match inner.strip_prefix('/') {
            Some(i) => (true, i),
            None => (false, inner),
        };
        let letter = inner.chars().next().unwrap();
        let id: String = inner.chars().skip(1).collect();
        let kind = if letter == 'x' || letter == 'p' {
            TagKind::Standalone
        } else if close {
            TagKind::Close
        } else {
            TagKind::Open
        };
        out.push(Inline::tag(id, kind, ""));
    }
    if !rest.is_empty() {
        out.push(Inline::text(rest));
    }
    out
}

/// Sets targets by unit id with LocOS token text.
pub fn translate(doc: &mut Document, pairs: &[(&str, String)]) {
    for (id, text) in pairs {
        let u = doc.units.iter_mut().find(|u| u.id == *id).unwrap_or_else(|| panic!("no unit {id}"));
        u.segments[0].target = from_tok(text);
        u.segments[0].state = SegState::Draft;
    }
}

pub fn merge(doc: &Document) -> Vec<u8> {
    Idml.merge(doc).unwrap()
}

pub fn units_of(data: &[u8]) -> Vec<String> {
    texts(&extract(data))
}

pub fn story_tree(data: &[u8], sid: &str) -> Tree {
    let name = format!("Stories/Story_{sid}.xml");
    let e = members_of(data).into_iter().find(|e| e.name == name).unwrap();
    Tree::parse(std::str::from_utf8(&e.data).unwrap(), &ParseLimits::default()).unwrap()
}

pub fn raw_story(data: &[u8], sid: &str) -> String {
    let name = format!("Stories/Story_{sid}.xml");
    let e = members_of(data).into_iter().find(|e| e.name == name).unwrap();
    String::from_utf8(e.data).unwrap()
}

pub fn named(t: &Tree, name: &str) -> Vec<Id> {
    t.iter(t.root()).into_iter().filter(|i| t.is_element(*i) && t.local(*i) == name && *i != t.root()).collect()
}

pub fn named_attr(t: &Tree, name: &str, attr: &str, val: &str) -> Vec<Id> {
    named(t, name).into_iter().filter(|i| t.attr(*i, attr) == Some(val)).collect()
}

/// What InDesign would show: the Content texts joined by `|`.
pub fn visible_text(t: &Tree) -> String {
    named(t, "Content").into_iter().map(|c| t.itertext(c)).collect::<Vec<_>>().join("|")
}
