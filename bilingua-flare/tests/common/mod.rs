//! Fixtures and helpers shared by the test files: the hand-built project of the LocOS suite (zipped at run time),
//! readers for the members of a result, and an independent well-formedness check.
#![allow(dead_code)]

use bilingua_flare::Flare;
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use std::io::{Cursor, Read, Write};

pub const JA: &str = "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}";
pub const AR: &str = "\u{0646}\u{0635} \u{0639}\u{0631}\u{0628}\u{064a}";

pub const TOPIC: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<html xmlns:MadCap="http://www.madcapsoftware.com/Schemas/MadCap.xsd" lang="en-us" xml:lang="en-us">
    <head><title>Printing guide</title>
    <link href="../Resources/Stylesheets/Styles.css" rel="stylesheet" type="text/css" />
    </head>
    <body>
        <h1>Print with <MadCap:variable name="General.ProductName" /></h1>
        <p MadCap:conditions="Default.PrintOnly">Install <MadCap:variable name="General.ProductName">Acme Writer</MadCap:variable> before you print, don't skip it.</p>
        <p>Read the <MadCap:glossaryTerm glossTerm="Glossary.Term0">spooler</MadCap:glossaryTerm> notes and see <MadCap:xref href="Other.htm">Other topic</MadCap:xref>.<MadCap:keyword term="Printing;Printing:Color;\:Colon start" /></p>
        <p>Text for <MadCap:conditionalText MadCap:conditions="Default.Online">online readers only</MadCap:conditionalText> and everyone.</p>
        <p>Brand line: <MadCap:snippetText src="../Resources/Snippets/Brand.flsnp" /> ends here.</p>
        <MadCap:snippetBlock src="../Resources/Snippets/Outer.flsnp" />
        <MadCap:dropDown>
            <MadCap:dropDownHead>
                <MadCap:dropDownHotspot>More details</MadCap:dropDownHotspot>
            </MadCap:dropDownHead>
            <MadCap:dropDownBody>
                <p>Hidden body text.</p>
            </MadCap:dropDownBody>
        </MadCap:dropDown>
        <p><img src="../Resources/Images/logo.png" alt="Company logo" /> Spacing&#160;kept.</p>
    </body>
</html>
"##;

pub fn intl() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns:MadCap=\"http://www.madcapsoftware.com/Schemas/MadCap.xsd\">\n<body>\r\n<p>{JA}</p>\r\n<p dir=\"rtl\">{AR} <MadCap:variable name=\"General.ProductName\" /></p>\r\n</body>\r\n</html>\r\n"
    )
}

pub const OUTER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns:MadCap="http://www.madcapsoftware.com/Schemas/MadCap.xsd">
<body>
<p>Outer snippet text.</p>
<MadCap:snippetBlock src="Inner.flsnp" />
</body>
</html>
"#;

pub const INNER: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns:MadCap="http://www.madcapsoftware.com/Schemas/MadCap.xsd">
<body>
<p>Inner snippet with <MadCap:conditionalText MadCap:conditions="Default.Print">print words</MadCap:conditionalText>.</p>
</body>
</html>
"#;

pub const BRAND: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns:MadCap="http://www.madcapsoftware.com/Schemas/MadCap.xsd">
<body><p>Acme makes writing tools</p></body>
</html>
"#;

pub const FLVAR: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<CatapultVariableSet>
  <Variable Name="ProductName" Comment="Do not abbreviate" EvaluatedDefinition="Acme Writer">Acme Writer</Variable>
  <Variable Name="Tagline" EvaluatedDefinition="Write &amp; ship">Write &amp; ship</Variable>
  <Variable Name="Year" EvaluatedDefinition="2026">2026</Variable>
  <Variable Name="Updated" Type="DateTime" Format="MMMM dd, yyyy" EvaluatedDefinition="October 10, 2026">October 10, 2026</Variable>
</CatapultVariableSet>
"#;

pub const FLTOC: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<CatapultToc Version="1">
  <TocEntry Title="Getting started" Link="/Content/Topic.htm">
    <TocEntry Title="[%=System.LinkedTitle%]" Link="/Content/Intl.htm" />
    <TocEntry Title="Guide to [%=General.ProductName%] printing" Link="/Content/Topic.htm" MadCap:conditions="Default.PrintOnly" xmlns:MadCap="http://www.madcapsoftware.com/Schemas/MadCap.xsd" />
  </TocEntry>
</CatapultToc>
"#;

pub const FLGLO: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<CatapultGlossary>
  <GlossaryEntry glossTerm="Glossary.Term0">
    <Terms>
      <Term>spooler</Term>
    </Terms>
    <Definition Link="">A queue that holds print jobs.</Definition>
  </GlossaryEntry>
</CatapultGlossary>
"#;

pub const FLCTS: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<CatapultConditionTagSet>
  <ConditionTag Name="PrintOnly" BackgroundColor="#ff0000" />
  <ConditionTag Name="Online" BackgroundColor="#00ff00" />
</CatapultConditionTagSet>
"##;

pub const FLPRJ: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<CatapultProject Version=\"1\" xml:lang=\"en-us\" />\n";

pub fn png() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    for _ in 0..4 {
        v.extend(0..=255u8);
    }
    v
}

pub type Members = Vec<(String, Vec<u8>)>;

pub fn base_members() -> Members {
    let m = |n: &str, d: &[u8]| (n.to_string(), d.to_vec());
    let mut intl_bytes = vec![0xEF, 0xBB, 0xBF];
    intl_bytes.extend_from_slice(intl().as_bytes());
    vec![
        m("Content/", b""),
        m("Content/Topic.htm", TOPIC.as_bytes()),
        m("Content/Intl.htm", &intl_bytes),
        m("Content/Resources/Snippets/Outer.flsnp", OUTER.as_bytes()),
        m("Content/Resources/Snippets/Inner.flsnp", INNER.as_bytes()),
        m("Content/Resources/Snippets/Brand.flsnp", BRAND.as_bytes()),
        m("Content/Resources/Stylesheets/Styles.css", b"p { color: #333; }\n"),
        m("Content/Resources/Images/logo.png", &png()),
        m("Project/VariableSets/General.flvar", FLVAR.as_bytes()),
        m("Project/TOCs/Master.fltoc", FLTOC.as_bytes()),
        m("Project/Glossaries/Glossary.flglo", FLGLO.as_bytes()),
        m("Project/ConditionTagSets/Default.flcts", FLCTS.as_bytes()),
        m("Help.flprj", FLPRJ.as_bytes()),
    ]
}

/// Zips `members` (a name ending in `/` is a folder; `.png` is stored, the rest deflated) with the time stamp
/// 2026-10-01 09:30:00.
pub fn make_zip(members: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let _ = w.set_raw_comment(b"flare fixture".to_vec().into_boxed_slice());
    let dt = zip::DateTime::from_date_and_time(2026, 10, 1, 9, 30, 0).unwrap();
    for (n, d) in members {
        let method = if n.ends_with(".png") { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
        let o = zip::write::SimpleFileOptions::default().compression_method(method).last_modified_time(dt);
        if n.ends_with('/') {
            w.add_directory(n.as_str(), o).unwrap();
        } else {
            w.start_file(n.as_str(), o).unwrap();
            w.write_all(d).unwrap();
        }
    }
    w.finish().unwrap().into_inner()
}

pub fn project() -> Vec<u8> {
    make_zip(&base_members())
}

/// The default members with `over` replacing (or appending), `drop` removing.
pub fn project_with(over: &[(&str, &[u8])], drop: &[&str]) -> Vec<u8> {
    let mut members = base_members();
    for (n, d) in over {
        match members.iter_mut().find(|(k, _)| k == n) {
            Some(m) => m.1 = d.to_vec(),
            None => members.push((n.to_string(), d.to_vec())),
        }
    }
    members.retain(|(n, _)| !drop.contains(&n.as_str()));
    make_zip(&members)
}

#[derive(Debug, Clone)]
pub struct Info {
    pub name: String,
    pub stored: bool,
    pub modified: Option<zip::DateTime>,
}

/// Member name to bytes (in zip order), plus the storage of the members.
pub fn members(data: &[u8]) -> (Vec<(String, Vec<u8>)>, Vec<Info>) {
    let mut z = zip::ZipArchive::new(Cursor::new(data)).expect("a zip");
    let mut list = Vec::new();
    let mut infos = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut d = Vec::new();
        f.read_to_end(&mut d).unwrap();
        let name = f.name().unwrap().into_owned();
        infos.push(Info { name: name.clone(), stored: f.compression() == zip::CompressionMethod::Stored, modified: f.last_modified() });
        list.push((name, d));
    }
    (list, infos)
}

pub fn get<'a>(m: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
    &m.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no member {name}")).1
}

pub fn text_of(m: &[(String, Vec<u8>)], name: &str) -> String {
    let raw = get(m, name);
    String::from_utf8(raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw).to_vec()).unwrap()
}

/// Every XML member parses with quick-xml (matching end tags, entities resolvable).
pub fn well_formed(m: &[(String, Vec<u8>)]) {
    for (name, raw) in m {
        let low = name.to_lowercase();
        if ![".htm", ".html", ".xhtml", ".flsnp", ".flvar", ".fltoc", ".flglo"].iter().any(|e| low.ends_with(e)) {
            continue;
        }
        let text = String::from_utf8(raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw).to_vec()).unwrap();
        use quick_xml::events::Event;
        let mut r = quick_xml::Reader::from_str(&text);
        let mut depth = 0i32;
        loop {
            match r.read_event() {
                Ok(Event::Eof) => break,
                Ok(Event::Start(_)) => depth += 1,
                Ok(Event::End(_)) => depth -= 1,
                Ok(Event::GeneralRef(g)) => {
                    let n = g.decode().unwrap();
                    assert!(n.starts_with('#') || matches!(n.as_ref(), "amp" | "lt" | "gt" | "quot" | "apos"), "{name}: undefined entity &{n};");
                }
                Ok(_) => {}
                Err(e) => panic!("{name} is not well-formed: {e}\n{text}"),
            }
        }
        assert_eq!(depth, 0, "{name}");
    }
}

pub fn extract(bytes: &[u8]) -> Document {
    Flare.extract(bytes, &ExtractOptions::default()).unwrap()
}

pub fn extract_err(bytes: &[u8]) -> Error {
    Flare.extract(bytes, &ExtractOptions::default()).expect_err("expected a refusal")
}

/// The unit text as LocOS shows it (inline codes as tokens).
pub fn loc_text(u: &Unit) -> String {
    to_locos_text(&u.source()).0
}

pub fn loc_texts(d: &Document) -> Vec<String> {
    d.units.iter().map(loc_text).collect()
}

/// Sets the target of unit number `i` (LocOS `p_index`) from LocOS text.
pub fn set(doc: &mut Document, i: usize, text: &str) {
    let u = &doc.units[i];
    let (_, map) = to_locos_text(&u.source());
    let inl = from_locos_text(text, &map);
    let id = u.segments[0].id.clone();
    doc.set_target(&id, inl).unwrap();
}

/// "T(" and ")" around every unit, as the LocOS suite does.
pub fn translate_all(doc: &mut Document) {
    for i in 0..doc.units.len() {
        if !doc.units[i].translate {
            continue;
        }
        let t = format!("T({})", loc_text(&doc.units[i]));
        set(doc, i, &t);
    }
}

pub fn merge(doc: &Document) -> Vec<u8> {
    Flare.merge(doc).unwrap()
}

pub fn index_of(d: &Document, text: &str) -> usize {
    d.units.iter().position(|u| loc_text(u) == text).unwrap_or_else(|| panic!("no unit {text:?} in {:?}", loc_texts(d)))
}
