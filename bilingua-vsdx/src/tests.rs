//! The LocOS suite (`tests/verify_vsdx_filter.py`), ported case by case, plus the checks this crate adds.

use crate::fixtures::*;
use crate::*;
use bilingua_zip::{read_zip, RawEntry};

pub fn extract_for(bytes: &[u8], target: Option<&str>) -> Document {
    Vsdx.extract(bytes, &ExtractOptions { target_lang: target.map(Into::into), ..Default::default() }).unwrap()
}

pub fn extract(bytes: &[u8]) -> Document {
    extract_for(bytes, None)
}

pub fn members(bytes: &[u8]) -> Vec<RawEntry> {
    read_zip("vsdx", bytes, &ZipLimits::default()).unwrap()
}

pub fn part(bytes: &[u8], name: &str) -> String {
    String::from_utf8(members(bytes).into_iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no part {name}")).data).unwrap()
}

pub fn text_of(doc: &Document, id: &str) -> String {
    let u = doc.units.iter().find(|u| u.id == id).unwrap_or_else(|| panic!("no unit {id}"));
    locos::to_locos_text(&u.source()).0
}

/// Sets a target written with LocOS tokens (`⟦x1⟧`).
pub fn set(doc: &mut Document, id: &str, text: &str) {
    let u = doc.units.iter().find(|u| u.id == id).unwrap_or_else(|| panic!("no unit {id}")).clone();
    let (_, map) = locos::to_locos_text(&u.source());
    let inl = locos::from_locos_text(text, &map);
    doc.set_target(&u.segments[0].id, inl).unwrap();
}

pub fn assert_same_entries(a: &[u8], b: &[u8]) {
    let (ea, eb) = (members(a), members(b));
    assert_eq!(ea.len(), eb.len());
    for (x, y) in ea.iter().zip(eb.iter()) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.data, y.data, "{} changed", x.name);
        assert_eq!(x.meta.stored, y.meta.stored);
        assert_eq!(x.meta.modified, y.meta.modified);
    }
}

pub fn changed_names(a: &[u8], b: &[u8]) -> Vec<String> {
    let (ea, eb) = (members(a), members(b));
    assert_eq!(ea.iter().map(|e| &e.name).collect::<Vec<_>>(), eb.iter().map(|e| &e.name).collect::<Vec<_>>(), "member order changed");
    ea.iter().zip(eb.iter()).filter(|(x, y)| x.data != y.data).map(|(x, _)| x.name.clone()).collect()
}

// 1. registration
#[test]
fn registration() {
    assert_eq!(Vsdx.id(), "vsdx");
    assert!(Vsdx.extensions().contains(&"vsdx") && Vsdx.extensions().contains(&"vsdm"));
    assert!(Vsdx.sniff(&package()));
    assert!(!Vsdx.sniff(b"PK\x03\x04 [Content_Types].xml word/"));
    assert!(!Vsdx.sniff(b"hello"));
}

// 2. walk and tokens
#[test]
fn walk_and_tokens() {
    let doc = extract(&package());
    let got: Vec<(String, String, String)> =
        doc.units.iter().map(|u| (u.id.clone(), locos::to_locos_text(&u.source()).0, u.context.style.clone().unwrap())).collect();
    let want = [
        ("name-1", "Overview", "Visio page name"),
        ("name-2", "Process details", "Visio page name"),
        // page2.xml first: pages.xml order through the relationships
        ("page2-1", "Start", "Visio shape text"),
        ("page1-1", "Process owner", "Visio shape data label"),
        ("page1-2", "Review the ⟦x1⟧request⟦x2⟧ by %1$s", "Visio shape text"),
        ("page1-3", "Printed on ⟦x1⟧", "Visio shape text"),
        // page1-4 (field only) is walked but is not a unit; edge white space stays outside the unit
        ("page1-5", "42", "Visio shape text"),
    ];
    let want: Vec<_> = want.iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect();
    assert_eq!(got, want);
    assert!(doc.units.iter().all(|u| !plain_text(&u.source()).contains("Master default")));
    // tags: point codes, all standalone, labelled
    let u = &doc.units[4];
    let tags: Vec<_> = u.segments[0]
        .source
        .iter()
        .filter_map(|i| if let Inline::Tag { kind, display, .. } = i { Some((*kind, display.clone())) } else { None })
        .collect();
    assert_eq!(tags, vec![(TagKind::Standalone, "<cp IX='1'/>".to_string()), (TagKind::Standalone, "<cp IX='0'/>".to_string())]);
    assert_eq!(u.codes["1"].label.as_deref(), Some("character properties"));
    let f = &doc.units[5];
    assert_eq!(f.codes["1"].label.as_deref(), Some("field"));
    assert_eq!(f.context.location.as_deref(), Some("page 2 \"Process details\", shape 3"));
    assert_eq!(doc.units[0].context.kind.as_deref(), Some("page-name"));
}

// 3. identity
#[test]
fn identity() {
    let src = package();
    let doc = extract(&src);
    assert_same_entries(&src, &Vsdx.merge(&doc).unwrap());
    // targets equal to the sources change nothing
    let mut d2 = doc.clone();
    for u in &mut d2.units {
        let s = u.segments[0].source.clone();
        u.segments[0].target = s;
    }
    assert_same_entries(&src, &Vsdx.merge(&d2).unwrap());
    // with a target language and nothing translated, still the original
    for l in ["de-DE", "he-IL"] {
        let doc = extract_for(&src, Some(l));
        assert_same_entries(&src, &Vsdx.merge(&doc).unwrap());
    }
}

// 4. gate: NameU kept on rename, Character LangID stamped on export
#[test]
fn nameu_and_langid() {
    let pg = pages().replace("NameU='Page-1' ", "");
    let page2 = page2().replace(
        "<Shape ID='7' Type='Shape'><Text>",
        "<Shape ID='7' Type='Shape'><Section N='Character'><Row IX='0'><Cell N='LangID' V='1033'/></Row><Row IX='1'><Cell N='LangID' V='1033' F='Inh'/></Row></Section><Text>",
    );
    let src = Pkg { pages: pg, page2, ..Default::default() }.build();
    let mut doc = extract_for(&src, Some("de-DE"));
    set(&mut doc, "name-2", "Prozessdetails");
    set(&mut doc, "page2-1", "Anfang");
    let out = Vsdx.merge(&doc).unwrap();
    let pages = part(&out, "visio/pages/pages.xml");
    assert!(pages.contains("NameU='Process details'"), "{pages}");
    assert!(pages.contains("Name='Prozessdetails'"), "{pages}");
    assert!(pages.contains("NameU='Overview'") && pages.contains("Name='Overview'"));
    let p2 = part(&out, "visio/pages/page2.xml");
    assert!(p2.contains("<Cell N='LangID' V='1031'/>"), "{p2}");
    assert!(p2.contains("V='1033' F='Inh'"), "a formula cell is left alone");
    // without a target language nothing is stamped
    let mut plain = extract(&src);
    set(&mut plain, "page2-1", "Anfang");
    assert!(part(&Vsdx.merge(&plain).unwrap(), "visio/pages/page2.xml").contains("V='1033'/>"));
    // a page that has NameU keeps it and gains nothing
    let mut d3 = extract(&package());
    set(&mut d3, "name-1", "Uebersicht");
    let pg3 = part(&Vsdx.merge(&d3).unwrap(), "visio/pages/pages.xml");
    assert!(pg3.contains("NameU='Overview' Name='Uebersicht' ViewScale='1'>"), "{pg3}");
}

pub fn rtl_page1_pub() -> String {
    rtl_page1()
}

fn rtl_page1() -> String {
    page1()
        .replace(
            "<Shape ID='1' Type='Shape' Master='2'><Cell N='PinX' V='1'/>",
            "<Shape ID='1' Type='Shape' Master='2'><Cell N='PinX' V='1'/><Section N='Character'><Row IX='0'><Cell N='LangID' V='en-US'/></Row></Section><Section N='Paragraph'><Row IX='0'><Cell N='HorzAlign' V='0'/><Cell N='Flags' V='0'/></Row><Row IX='1'><Cell N='Flags' V='0' F='GUARD(0)'/></Row><Row IX='3'><Cell N='SpLine' V='-1.2'/></Row></Section>",
        )
        .replace("request<cp IX='0'/>", "request<cp IX='0'/><pp IX='1'/><pp IX='2'/>")
}

fn para(xml: &str, shape: &str) -> String {
    let start = xml.find(&format!("<Shape ID='{shape}'")).unwrap();
    let body = &xml[start..start + xml[start..].find("</Shape>").unwrap()];
    match body.find("<Section N='Paragraph'>") {
        None => String::new(),
        Some(s) => body[s..s + body[s..].find("</Section>").unwrap()].to_string(),
    }
}

// 5. #740: a right-to-left target sets Paragraph Flags
#[test]
fn rtl_paragraph_flags() {
    let src = Pkg { page1: rtl_page1(), ..Default::default() }.build();
    for loc in ["he-IL", "ar-SA", "fa"] {
        let mut doc = extract_for(&src, Some(loc));
        set(&mut doc, "page2-1", "התחלה");
        set(&mut doc, "page1-2", "סקור את ⟦x1⟧הבקשה⟦x2⟧ על ידי %1$s");
        let out = Vsdx.merge(&doc).unwrap();
        let p1 = part(&out, "visio/pages/page1.xml");
        let pa = para(&p1, "1");
        assert!(pa.contains("<Row IX='0'><Cell N='HorzAlign' V='0'/><Cell N='Flags' V='1'/></Row>"), "{loc} {pa}");
        assert!(pa.contains("<Row IX='1'><Cell N='Flags' V='0' F='GUARD(0)'/></Row>"), "{loc} {pa}");
        assert!(pa.contains("<Row IX='2'><Cell N='Flags' V='1'/></Row><Row IX='3'>"), "{loc} {pa}");
        assert!(pa.contains("<Row IX='3'><Cell N='SpLine' V='-1.2'/></Row>"), "row 3 is unused: kept");
        assert!(!p1.contains("RTLText"));
        assert_eq!(para(&p1, "5"), "", "an untranslated shape gets nothing");
        let p2 = part(&out, "visio/pages/page2.xml");
        assert!(p2.contains("<Shape ID='7' Type='Shape'><Section N='Paragraph'><Row IX='0'><Cell N='Flags' V='1'/></Row></Section><Text>התחלה</Text>"), "{loc} {p2}");
        // the Character LangID the shape had is the target's too
        assert!(!p1.contains("V='en-US'"), "{p1}");
        assert_eq!(changed_names(&src, &out), vec!["visio/pages/page1.xml", "visio/pages/page2.xml"], "{loc}");
    }
    // left-to-right targets and no target language write no direction
    for t in [Some("de-DE"), None] {
        let mut doc = extract_for(&src, t);
        set(&mut doc, "page2-1", "Anfang");
        set(&mut doc, "page1-2", "Prüfen ⟦x1⟧Anfrage⟦x2⟧");
        let out = Vsdx.merge(&doc).unwrap();
        assert!(!part(&out, "visio/pages/page1.xml").contains("<Cell N='Flags' V='1'/>"));
        assert!(!part(&out, "visio/pages/page2.xml").contains("N='Paragraph'"));
    }
    // gate: F="Inh" is only inherited, so it becomes a local 1; rows out of order, a missing row goes after
    // the highest lower IX
    let gate = rtl_page1().replace(
        "<Section N='Paragraph'><Row IX='0'><Cell N='HorzAlign' V='0'/><Cell N='Flags' V='0'/></Row><Row IX='1'><Cell N='Flags' V='0' F='GUARD(0)'/></Row><Row IX='3'><Cell N='SpLine' V='-1.2'/></Row></Section>",
        "<Section N='Paragraph'><Row IX='3'><Cell N='SpLine' V='-1.2'/></Row><Row IX='0'><Cell N='Flags' V='0' F='Inh'/></Row><Row IX='1'><Cell N='HorzAlign' V='1'/></Row></Section>",
    );
    let src = Pkg { page1: gate, ..Default::default() }.build();
    let mut doc = extract_for(&src, Some("he-IL"));
    set(&mut doc, "page1-2", "סקור ⟦x1⟧הבקשה⟦x2⟧");
    let pa = para(&part(&Vsdx.merge(&doc).unwrap(), "visio/pages/page1.xml"), "1");
    assert!(pa.contains("<Row IX='0'><Cell N='Flags' V='1'/></Row>"), "{pa}");
    assert!(!pa.contains("Inh"), "{pa}");
    assert!(pa.contains("<Row IX='1'><Cell N='HorzAlign' V='1'/><Cell N='Flags' V='1'/></Row><Row IX='2'><Cell N='Flags' V='1'/></Row>"), "{pa}");
    assert!(pa.find("<Row IX='3'>").unwrap() < pa.find("<Row IX='0'>").unwrap(), "existing order kept");
}

// 6. round trip
#[test]
fn round_trip() {
    let src = Pkg { vba: true, ..Default::default() }.build();
    let mut doc = extract(&src);
    set(&mut doc, "name-2", "Prozessdetails");
    set(&mut doc, "page1-1", "Prozessverantwortlicher");
    set(&mut doc, "page1-2", "Prüfen Sie die ⟦x1⟧Anfrage⟦x2⟧ bis %1$s");
    set(&mut doc, "page1-3", "Gedruckt am ⟦x1⟧ <ok> & fertig");
    let out = Vsdx.merge(&doc).unwrap();
    assert_eq!(changed_names(&src, &out), vec!["visio/pages/pages.xml", "visio/pages/page1.xml"]);
    let p1 = part(&out, "visio/pages/page1.xml");
    assert!(p1.contains("<Text><cp IX='0'/><pp IX='0'/>Prüfen Sie die <cp IX='1'/>Anfrage<cp IX='0'/> bis %1$s\n</Text>"), "{p1}");
    assert!(p1.contains("Gedruckt am <fld IX='0'>10/10/2026</fld> &lt;ok&gt; &amp; fertig\n</Text>"), "{p1}");
    assert!(p1.contains("V='Prozessverantwortlicher'") && p1.contains("F='Sheet.5!Prop.X'"));
    let pg = part(&out, "visio/pages/pages.xml");
    assert!(pg.contains("NameU='Page-1' Name='Prozessdetails'") && pg.contains("NameU='Overview'"));
    // markup the filter did not touch keeps its own declaration and line ends
    assert!(pg.starts_with("<?xml version='1.0' encoding='utf-8' ?>\r\n"), "{}", &pg[..60]);
    let again = extract(&out);
    for u in &doc.units {
        let want = plain_text(&u.output());
        let got = again.units.iter().find(|x| x.id == u.id).unwrap();
        assert_eq!(plain_text(&got.source()), want, "{}", u.id);
        if u.id == "page1-2" {
            assert_eq!(got.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count(), 2);
        }
    }
    assert_eq!(text_of(&again, "name-1"), "Overview");
    assert_eq!(text_of(&again, "page2-1"), "Start");
}

// 7. token damage
#[test]
fn token_damage() {
    let src = package();
    let mut doc = extract(&src);
    let tag = |n: &str| Inline::tag(n, TagKind::Standalone, "");
    // x2 first use only, foreign tokens dropped, missing x1 appended before the suffix
    doc.units[4].segments[0].target = vec![tag("2"), Inline::text("Nur"), tag("2"), Inline::text(" Text "), tag("7")];
    let p1 = part(&Vsdx.merge(&doc).unwrap(), "visio/pages/page1.xml");
    assert!(p1.contains("<pp IX='0'/><cp IX='0'/>Nur Text <cp IX='1'/>\n</Text>"), "{p1}");
    let mut doc = extract(&src);
    set(&mut doc, "page1-3", "Gedruckt");
    let p1 = part(&Vsdx.merge(&doc).unwrap(), "visio/pages/page1.xml");
    assert!(p1.contains("<Text>Gedruckt<fld IX='0'>10/10/2026</fld>\n</Text>"), "{p1}");
    let mut doc = extract(&src);
    doc.units[2].segments[0].target = vec![Inline::text("Bad \u{1} char\u{b}line")];
    assert!(part(&Vsdx.merge(&doc).unwrap(), "visio/pages/page2.xml").contains("<Text>Bad  char\nline</Text>"));
}

// 8. no pages.xml: page<N>.xml in numeric order
#[test]
fn no_pages_xml_fallback() {
    let src = zip_of(vec![
        ("visio/pages/page10.xml".into(), page2().replace("Start", "Ten").into_bytes()),
        ("visio/pages/page2.xml".into(), page2().replace("Start", "Two").into_bytes()),
    ]);
    let doc = extract(&src);
    let texts: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, vec!["Two", "Ten"]);
    assert_eq!(doc.units[0].id, "page2-1");
}

pub fn refused(bytes: &[u8], needle: &str) {
    match Vsdx.extract(bytes, &ExtractOptions::default()) {
        Err(e) => assert!(e.to_string().to_lowercase().contains(&needle.to_lowercase()), "{e}"),
        Ok(_) => panic!("not refused: {needle}"),
    }
}

// 9. hostile
#[test]
fn hostile() {
    let bomb = page2().replace("<?xml version='1.0' encoding='utf-8' ?>\r\n", "<?xml version='1.0'?><!DOCTYPE l [<!ENTITY a 'aaaa'>]>").replace("Start", "&a;");
    refused(&Pkg { page2: bomb, ..Default::default() }.build(), "xml");
    refused(&Pkg { page2: "<PageContents".into(), ..Default::default() }.build(), "not valid XML");
    refused(b"not a zip at all", "zip");
    refused(&zip_of(vec![("word/document.xml".into(), b"<x/>".to_vec())]), "no drawing pages");
    // zip bomb
    let zeros = Pkg { extra: vec![("visio/media/huge.bin".into(), vec![0u8; 3 * 1024 * 1024])], ..Default::default() }.build();
    match Vsdx.extract(&zeros, &ExtractOptions::default()) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::ZipRatio),
        o => panic!("{o:?}"),
    }
    // the running cap, then the markup budget before any parse
    let mut limits = Limits { max_units: 3, ..Default::default() };
    let o = ExtractOptions { limits: Some(limits), ..Default::default() };
    match Vsdx.extract(&package(), &o) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::Units),
        r => panic!("{r:?}"),
    }
    limits.max_units = 1;
    let o = ExtractOptions { limits: Some(limits), ..Default::default() };
    match Vsdx.extract(&package(), &o) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::XmlElements),
        r => panic!("{r:?}"),
    }
    let long = page2().replace("Start", &"x".repeat(100_001));
    match Vsdx.extract(&Pkg { page2: long, ..Default::default() }.build(), &ExtractOptions::default()) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::UnitChars),
        r => panic!("{r:?}"),
    }
}

// 10. upload and export of a .vsdm
#[test]
fn macro_enabled_export() {
    let src = Pkg { vba: true, ..Default::default() }.build();
    let mut doc = extract_for(&src, Some("de-DE"));
    assert!(doc.units.iter().any(|u| plain_text(&u.source()) == "Start"));
    set(&mut doc, "page2-1", "Beginn");
    let out = Vsdx.merge(&doc).unwrap();
    assert!(part(&out, "visio/pages/page2.xml").contains("<Text>Beginn</Text>"));
    let (a, b) = (members(&src), members(&out));
    let get = |m: &[RawEntry], n: &str| m.iter().find(|e| e.name == n).unwrap().data.clone();
    assert_eq!(get(&a, "visio/vbaProject.bin"), get(&b, "visio/vbaProject.bin"));
    assert_eq!(get(&a, "[Content_Types].xml"), get(&b, "[Content_Types].xml"));
}
