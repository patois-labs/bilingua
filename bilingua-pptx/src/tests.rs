use crate::fixtures::*;
use crate::xml;
use crate::zipio::{read_zip, ZipLimits};
use crate::*;

fn extract_for(bytes: &[u8], target: &str) -> Document {
    Pptx.extract(bytes, &ExtractOptions { target_lang: Some(target.into()), ..Default::default() }).unwrap()
}

fn extract(bytes: &[u8]) -> Document {
    extract_for(bytes, "es-ES")
}

fn part(bytes: &[u8], name: &str) -> String {
    let es = read_zip(bytes, &ZipLimits::default()).unwrap();
    String::from_utf8(es.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no part {name}")).data.clone()).unwrap()
}

fn unit<'a>(doc: &'a Document, id: &str) -> &'a Unit {
    doc.units.iter().find(|u| u.id == id).unwrap_or_else(|| panic!("no unit {id}"))
}

fn src_text(doc: &Document, id: &str) -> String {
    plain_text(&unit(doc, id).source())
}

/// Replaces the first piece of text; tags stay where they were.
fn set_plain(doc: &mut Document, id: &str, text: &str) {
    let u = unit(doc, id).clone();
    let mut done = false;
    let mut target = vec![];
    for i in u.source() {
        match i {
            Inline::Text(_) if !done => {
                done = true;
                target.push(Inline::text(text));
            }
            Inline::Text(_) => {}
            o => target.push(o),
        }
    }
    doc.set_target(&u.segments[0].id, target).unwrap();
}

fn assert_same_parts(a: &[u8], b: &[u8]) {
    let ea = read_zip(a, &ZipLimits::default()).unwrap();
    let eb = read_zip(b, &ZipLimits::default()).unwrap();
    assert_eq!(ea.len(), eb.len());
    for (x, y) in ea.iter().zip(eb.iter()) {
        assert_eq!(x.name, y.name);
        assert_eq!(x.data, y.data, "{} changed", x.name);
        assert_eq!(x.meta.stored, y.meta.stored);
        assert_eq!(x.meta.modified, y.meta.modified);
    }
}

fn assert_all_xml_valid(bytes: &[u8]) {
    for e in read_zip(bytes, &ZipLimits::default()).unwrap() {
        if e.name.ends_with(".xml") || e.name.ends_with(".rels") {
            let t = String::from_utf8(e.data).unwrap();
            xml::parse(&t).unwrap_or_else(|m| panic!("{} is not well-formed: {m}", e.name));
            assert!(!t.chars().any(|c| ('\u{E000}'..='\u{E004}').contains(&c)), "{} has a marker", e.name);
        }
    }
}

// In the fixture deck, slide2.xml is the first slide and slide1.xml the second.

#[test]
fn extracts_units_with_context() {
    let doc = extract(&presentation_zip());
    let t = unit(&doc, "slide1-p1");
    assert_eq!(plain_text(&t.source()), "Welcome to the demo");
    assert_eq!(t.context.kind.as_deref(), Some("title"));
    assert_eq!(t.context.location.as_deref(), Some("slide 2, shape \"Title 1\""));
    assert_eq!(unit(&doc, "slide1-p2").context.kind.as_deref(), Some("subtitle"));
    let b = unit(&doc, "slide1-p3");
    assert_eq!(b.context.kind.as_deref(), Some("body"));
    assert_eq!(b.context.heading.as_deref(), Some("Welcome to the demo"));
    assert_eq!(unit(&doc, "slide1-p14").context.kind.as_deref(), Some("textbox"));
    // Table cells: row and column.
    assert!(doc.units.iter().all(|u| u.id != "slide1-p18"), "a price is not text");
    let c = unit(&doc, "slide1-p16");
    assert_eq!(c.context.kind.as_deref(), Some("cell"));
    assert_eq!(c.context.location.as_deref(), Some("slide 2, shape \"Table 8\", row 1, column 2"));
    // Grouped shape.
    assert_eq!(src_text(&doc, "slide1-p19"), "Inside a group");
    // Slide number field, empty and numeric paragraphs are not units.
    for id in ["slide1-p20", "slide1-p21", "slide1-p22", "slide1-p23"] {
        assert!(doc.units.iter().all(|u| u.id != id), "{id}");
    }
    // Notes.
    let n = unit(&doc, "notesSlide1-p1");
    assert_eq!(n.context.kind.as_deref(), Some("notes"));
    assert_eq!(n.context.location.as_deref(), Some("slide 1, notes, shape \"Notes Placeholder 2\""));
    // SmartArt data.
    assert_eq!(src_text(&doc, "data1-p1"), "First step");
    // Layout: prompt text left out, real text kept. Master placeholders left out.
    assert!(doc.units.iter().all(|u| plain_text(&u.source()) != "Click to edit Master title style"));
    let l = doc.units.iter().find(|u| plain_text(&u.source()) == "Contoso Labs internal").unwrap();
    assert_eq!(l.context.location.as_deref(), Some("layout 1 \"Title Slide\", shape \"Brand line\""));
    assert!(doc.units.iter().all(|u| !u.id.starts_with("slideMaster")));
    // AlternateContent: only the Choice copy.
    assert_eq!(doc.units.iter().filter(|u| plain_text(&u.source()) == "Choice text").count(), 1);
    // Source language comes from the runs.
    assert_eq!(doc.source_lang.as_deref(), Some("en-US"));
    // Slide order follows the deck, not the file names.
    let first = doc.units.iter().position(|u| u.id == "slide2-p1").unwrap();
    let second = doc.units.iter().position(|u| u.id == "slide1-p1").unwrap();
    assert!(first < second);
}

#[test]
fn all_eleven_languages_are_extracted() {
    let doc = extract(&presentation_zip());
    for (i, (_, t)) in LINES.iter().enumerate() {
        assert_eq!(src_text(&doc, &format!("slide1-p{}", 3 + i)), *t);
    }
}

#[test]
fn runs_become_paired_tags_and_codes() {
    let doc = extract(&presentation_zip());
    let u = unit(&doc, "slide1-p14");
    let src = u.source();
    let shown: Vec<String> = src
        .iter()
        .map(|i| match i {
            Inline::Text(t) => format!("T:{t}"),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect();
    assert_eq!(
        shown,
        vec!["T:Plain start, ", "<b>", "T:bold middle", "</b>", "T: and an ", "<link>", "T:example link", "</link>", "<br/>", "T:second line tex", "{slidenum}"]
    );
}

#[test]
fn unchanged_presentation_is_byte_identical() {
    let bytes = presentation_zip();
    let doc = extract(&bytes);
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());
    let back: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
    assert_same_parts(&bytes, &Pptx.merge(&back).unwrap());
}

#[test]
fn every_primary_language_round_trips() {
    let bytes = presentation_zip();
    for (lang, sample) in LINES {
        let mut doc = extract_for(&bytes, lang);
        for id in ["slide1-p1", "slide1-p3", "slide1-p14", "slide1-p16", "notesSlide1-p1", "data1-p1", "slide2-p1"] {
            set_plain(&mut doc, id, &format!("{sample} {id}"));
        }
        let (out, report) = merge_with_report(&doc).unwrap();
        assert!(report.warnings.is_empty());
        assert_all_xml_valid(&out);
        let again = extract_for(&out, lang);
        assert_eq!(src_text(&again, "slide1-p1"), format!("{sample} slide1-p1"), "{lang}");
        assert_eq!(src_text(&again, "slide1-p3"), format!("{sample} slide1-p3"));
        assert_eq!(src_text(&again, "slide1-p16"), format!("{sample} slide1-p16"));
        assert_eq!(src_text(&again, "notesSlide1-p1"), format!("{sample} notesSlide1-p1"));
        assert_eq!(src_text(&again, "data1-p1"), format!("{sample} data1-p1"));
        // Untouched text stays.
        assert_eq!(src_text(&again, "slide1-p4"), LINES[1].1);
        // The language of rebuilt runs is the target language.
        let s = part(&out, "ppt/slides/slide1.xml");
        assert!(s.contains(&format!("<a:rPr lang=\"{lang}\"/><a:t>{sample} slide1-p1</a:t>")), "{lang}: {s}");
        // Right-to-left: only for Arabic.
        assert_eq!(s.contains("<a:pPr rtl=\"1\" lvl=\"0\"/>"), lang == "ar", "{lang}");
    }
}

#[test]
fn explicit_direction_is_kept() {
    let bytes = presentation_zip();
    let mut doc = extract_for(&bytes, "ar-SA");
    set_plain(&mut doc, "slide2-p3", "نص جديد");
    let (out, _) = merge_with_report(&doc).unwrap();
    let s = part(&out, "ppt/slides/slide2.xml");
    assert!(s.contains("<a:pPr rtl=\"1\" algn=\"r\"/><a:r><a:rPr lang=\"ar-SA\"/><a:t>نص جديد</a:t>"), "{s}");
    assert_eq!(s.matches("rtl=").count(), 1, "no second rtl attribute: {s}");
}

#[test]
fn rich_target_keeps_formatting_and_fields() {
    let bytes = presentation_zip();
    let mut doc = extract_for(&bytes, "de-DE");
    let u = unit(&doc, "slide1-p14").clone();
    let tags: Vec<Inline> = u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    // tags: open b, close b, open link, close link, br, field
    assert_eq!(tags.len(), 6);
    let target = vec![
        Inline::text("Normal, "),
        tags[0].clone(),
        Inline::text("fett"),
        tags[1].clone(),
        Inline::text(" und "),
        tags[2].clone(),
        Inline::text("Beispiellink"),
        tags[3].clone(),
        tags[4].clone(),
        Inline::text("zweite Zeile\nund dritte"),
        tags[5].clone(),
    ];
    doc.set_target(&u.segments[0].id, target).unwrap();
    let (out, _) = merge_with_report(&doc).unwrap();
    assert_all_xml_valid(&out);
    let s = part(&out, "ppt/slides/slide1.xml");
    assert!(s.contains("<a:r><a:rPr lang=\"de-DE\" b=\"1\"/><a:t>fett</a:t></a:r>"), "{s}");
    assert!(s.contains("<a:rPr lang=\"de-DE\"><a:hlinkClick r:id=\"rId9\"/></a:rPr><a:t>Beispiellink</a:t>"), "{s}");
    assert!(s.contains("<a:br><a:rPr lang=\"en-US\"/></a:br>"), "the original br is kept: {s}");
    assert!(s.contains("<a:t>zweite Zeile</a:t></a:r><a:br><a:rPr lang=\"de-DE\"/></a:br><a:r><a:rPr lang=\"de-DE\"/><a:t>und dritte</a:t>"), "{s}");
    assert!(s.contains("<a:fld id=\"{B1}\" type=\"slidenum\">"));
    // The err mark is dropped from the rebuilt run, dirty too.
    assert!(!s.contains("err=\"1\""));
    // The end of paragraph properties stay.
    assert!(s.contains("</a:fld><a:endParaRPr lang=\"en-US\"/></a:p>"));
    // A tag the source never had is refused.
    assert!(matches!(doc.set_target(&u.segments[0].id, vec![Inline::tag("77", TagKind::Open, "<b>")]), Err(Error::InventedTag(_))));
}

#[test]
fn smartart_drawing_follows_the_data() {
    let bytes = presentation_zip();
    let mut doc = extract(&bytes);
    set_plain(&mut doc, "data1-p2", "Segundo paso");
    let (out, _) = merge_with_report(&doc).unwrap();
    let d = part(&out, "ppt/diagrams/drawing1.xml");
    assert!(d.contains("<a:pPr algn=\"ctr\"/><a:r><a:rPr lang=\"es-ES\" sz=\"1800\"/><a:t>Segundo paso</a:t>"), "{d}");
    assert!(d.contains("<a:t>First step</a:t>"), "{d}");
    assert!(part(&out, "ppt/diagrams/data1.xml").contains("Segundo paso"));
    assert_all_xml_valid(&out);
}

#[test]
fn preview_has_one_section_per_slide_and_a_span_per_segment() {
    let mut doc = extract(&presentation_zip());
    set_plain(&mut doc, "slide1-p1", "Bienvenidos a la demo");
    let html = Pptx.preview(&doc).unwrap();
    assert_eq!(html.matches("<section class=\"bl-slide\"").count(), 3);
    assert!(html.contains("<h2><span data-bl-seg=\"slide1-p1\">Bienvenidos a la demo</span></h2>"), "{html}");
    assert!(html.contains("<table"));
    assert!(html.contains("<aside class=\"bl-notes\">"));
    assert!(html.contains("<b>bold middle</b>"));
    assert!(html.contains("<span class=\"bl-code\">{slidenum}</span>"));
    for (u, seg) in doc.segments() {
        assert!(html.contains(&format!("data-bl-seg=\"{}\"", seg.id)), "{} missing from the preview of {}", seg.id, u.id);
    }
    assert!(!html.contains("<script"));
    assert_eq!(Pptx.render_segment(&doc, "slide1-p1").unwrap(), "Bienvenidos a la demo");
    assert!(Pptx.render_segment(&doc, "nope").is_none());
    // Arabic target: right-to-left page.
    let doc = extract_for(&presentation_zip(), "ar");
    assert!(Pptx.preview(&doc).unwrap().starts_with("<!doctype html><html dir=\"rtl\">"));
}

#[test]
fn text_with_markup_characters_is_escaped() {
    let mut doc = extract(&presentation_zip());
    set_plain(&mut doc, "slide1-p1", "Tom & Jerry <b> \"quotes\"");
    let (out, _) = merge_with_report(&doc).unwrap();
    assert!(part(&out, "ppt/slides/slide1.xml").contains("Tom &amp; Jerry &lt;b&gt; \"quotes\""));
    assert_all_xml_valid(&out);
    let html = Pptx.preview(&doc).unwrap();
    assert!(html.contains("Tom &amp; Jerry &lt;b&gt; &quot;quotes&quot;"));
}

#[test]
fn bad_files_are_refused_not_panicked_on() {
    assert!(Pptx.extract(b"not a zip", &ExtractOptions::default()).is_err());
    assert!(Pptx.extract(b"", &ExtractOptions::default()).is_err());
    assert!(Pptx.extract(&zip(&[("hello.txt", "hi".into())]), &ExtractOptions::default()).is_err());
    assert!(Pptx.extract(&zip(&[("../evil.xml", "<a/>".into())]), &ExtractOptions::default()).is_err());
    // DOCTYPE in a slide.
    let mut files: Vec<(&str, String)> = vec![];
    for e in read_zip(&presentation_zip(), &ZipLimits::default()).unwrap() {
        let mut t = String::from_utf8(e.data).unwrap();
        if e.name == "ppt/slides/slide1.xml" {
            t = t.replace("<p:sld ", "<!DOCTYPE x [<!ENTITY y \"z\">]><p:sld ");
        }
        files.push((Box::leak(e.name.into_boxed_str()), t));
    }
    assert!(Pptx.extract(&zip(&files), &ExtractOptions::default()).is_err());
}

#[test]
fn broken_side_parts_are_left_alone() {
    let mut files: Vec<(&str, String)> = vec![];
    for e in read_zip(&presentation_zip(), &ZipLimits::default()).unwrap() {
        let mut t = String::from_utf8(e.data).unwrap();
        if e.name == "ppt/notesSlides/notesSlide1.xml" {
            t.push_str("<unclosed>");
        }
        files.push((Box::leak(e.name.into_boxed_str()), t));
    }
    let bytes = zip(&files);
    let doc = extract(&bytes);
    assert!(doc.units.iter().all(|u| u.id != "notesSlide1-p1"));
    assert_same_parts(&bytes, &Pptx.merge(&doc).unwrap());
}

#[test]
fn truncated_files_never_panic() {
    let bytes = presentation_zip();
    for cut in (0..bytes.len()).step_by(89) {
        let _ = Pptx.extract(&bytes[..cut], &ExtractOptions::default());
    }
}

#[test]
fn sniff_and_extensions() {
    assert!(Pptx.sniff(&presentation_zip()));
    assert!(!Pptx.sniff(b"PK\x03\x04 random"));
    assert!(Pptx.extensions().contains(&"pptx"));
}

#[test]
fn libreoffice_opens_the_merged_file_when_installed() {
    let candidates = [
        "C:\\Program Files\\LibreOffice\\program\\soffice.exe",
        "C:\\Program Files (x86)\\LibreOffice\\program\\soffice.exe",
        "/usr/bin/soffice",
        "/usr/local/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ];
    let Some(soffice) = candidates.iter().find(|p| std::path::Path::new(p).exists()) else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    let mut doc = extract_for(&presentation_zip(), "ja-JP");
    for (i, u) in doc.units.clone().iter().enumerate() {
        let (_, sample) = LINES[i % LINES.len()];
        let t: Vec<Inline> = u
            .source()
            .iter()
            .map(|x| match x {
                Inline::Text(_) => Inline::text(format!("{sample} {i}")),
                o => o.clone(),
            })
            .collect();
        doc.set_target(&u.segments[0].id, t).unwrap();
    }
    let (out, _) = merge_with_report(&doc).unwrap();
    let dir = std::env::temp_dir().join(format!("bilingua-pptx-lo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("merged.pptx");
    std::fs::write(&file, out).unwrap();
    let profile = dir.join("profile");
    let url = format!("-env:UserInstallation=file:///{}", profile.to_string_lossy().replace('\\', "/"));
    let status = std::process::Command::new(soffice)
        .arg(url)
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(&dir)
        .arg(&file)
        .output()
        .unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    assert!(dir.join("merged.pdf").exists(), "{}{}", String::from_utf8_lossy(&status.stdout), String::from_utf8_lossy(&status.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}
