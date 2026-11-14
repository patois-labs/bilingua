//! Regression tests for the DOCX fixes of the 0.9.1 security audit: a comment copied into many units counts
//! against the context budget, and the XML guard and the paragraph count cover the parts that are parsed, whatever
//! their entry names.

use bilingua_docx::Docx;
use bilingua_model::{Document, Error, ExtractOptions, Filter, LimitKind, Limits};
use bilingua_zip::{write_zip, EntryMeta};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MAIN_CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const RT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn zip(entries: &[(&str, String)]) -> Vec<u8> {
    let parts: Vec<(EntryMeta, Vec<u8>)> = entries
        .iter()
        .map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.clone().into_bytes()))
        .collect();
    write_zip(&parts).unwrap()
}

fn limit_kind(r: Result<Document, Error>) -> LimitKind {
    match r {
        Err(Error::Limit(e)) => e.kind,
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(d) => panic!("expected a limit error, the file was accepted with {} units", d.units.len()),
    }
}

fn locos() -> ExtractOptions {
    ExtractOptions::new().with_profile("locos")
}

/// A package whose main part is `main` (named by the content types and by the package relationship), holding
/// `body` inside `w:body`.
fn package(main: &str, body: &str, more: &[(&str, String)]) -> Vec<u8> {
    let ct = format!(
        "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"bin\" ContentType=\"application/octet-stream\"/><Override PartName=\"/{main}\" ContentType=\"{MAIN_CT}\"/></Types>"
    );
    let rels = format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{RT}/officeDocument\" Target=\"{main}\"/></Relationships>"
    );
    let doc = format!("<w:document xmlns:w=\"{W}\"><w:body>{body}</w:body></w:document>");
    let mut entries = vec![("[Content_Types].xml", ct), ("_rels/.rels", rels), (main, doc)];
    entries.extend(more.iter().cloned());
    zip(&entries)
}

/// One comment referenced by every paragraph: each copy on a unit is charged against `max_context_bytes`.
#[test]
fn comment_copies_are_budgeted() {
    let comments = format!(
        "<w:comments xmlns:w=\"{W}\"><w:comment w:id=\"0\"><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:comment></w:comments>",
        "a".repeat(2000)
    );
    let body = "<w:p><w:r><w:commentReference w:id=\"0\"/></w:r><w:r><w:t>x</w:t></w:r></w:p>".repeat(200);
    let bytes = package("word/document.xml", &body, &[("word/comments.xml", comments)]);
    // 200 copies of a 2000-letter comment are 400 KB of notes: over a 100 KB budget, inside the default
    let tight = ExtractOptions::new().with_limits(Limits::new().with_max_context_bytes(100_000));
    assert_eq!(limit_kind(Docx.extract(&bytes, &tight)), LimitKind::Context);
    let doc = Docx.extract(&bytes, &ExtractOptions::new()).unwrap();
    let noted = doc.units.iter().filter(|u| u.context.notes.iter().any(|n| n.starts_with("Comment: aaa"))).count();
    assert_eq!(noted, 200);
}

/// The main part is chosen by content type and relationship, so its name may end in `.bin`, sit outside `word/`
/// or under `customXml/`: the element cap and the paragraph cap apply all the same, on both paths.
#[test]
fn guards_follow_the_parsed_main_part() {
    let body = "<w:p/>".repeat(5000);
    let elements = Limits::new().with_max_xml_elements(1000);
    let paragraphs = Limits::new().with_max_paragraphs(1000);
    for main in ["word/document.xml", "word/document.bin", "doc/main.xml", "customXml/main.xml"] {
        let bytes = package(main, &body, &[]);
        for (path, opts) in [("native", ExtractOptions::new()), ("locos", locos())] {
            let what = format!("{main} ({path})");
            assert_eq!(limit_kind(Docx.extract(&bytes, &opts.clone().with_limits(elements))), LimitKind::XmlElements, "{what}");
            assert_eq!(limit_kind(Docx.extract(&bytes, &opts.clone().with_limits(paragraphs))), LimitKind::Paragraphs, "{what}");
        }
    }
}

/// An ordinary small document under an unusual main-part name still extracts, with the same units on each path.
#[test]
fn an_unusual_main_part_name_still_extracts() {
    let body = "<w:p><w:r><w:t>Hello there.</w:t></w:r></w:p>";
    for (path, opts) in [("native", ExtractOptions::new()), ("locos", locos())] {
        let usual = Docx.extract(&package("word/document.xml", body, &[]), &opts).unwrap();
        assert_eq!(usual.units.len(), 1, "{path}");
        for main in ["word/document.bin", "doc/main.xml", "customXml/main.xml"] {
            let doc = Docx.extract(&package(main, body, &[]), &opts).unwrap_or_else(|e| panic!("{main} ({path}): {e}"));
            assert_eq!(doc.units.len(), 1, "{main} ({path})");
        }
    }
}

/// Under the LocOS profile a footnotes part is found by relationship and parsed when the walk reaches it: one the
/// name-keyed count does not see (`word/notes.bin`) is counted before its tree is built.
#[test]
fn a_part_opened_by_relationship_is_guarded() {
    let rels = format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{RT}/footnotes\" Target=\"notes.bin\"/></Relationships>"
    );
    let notes = |n: usize| format!("<w:footnotes xmlns:w=\"{W}\"><w:footnote w:id=\"1\">{}</w:footnote></w:footnotes>", "<w:p/>".repeat(n));
    let body = "<w:p><w:r><w:t>Hello there.</w:t></w:r></w:p>";
    let pkg = |n: usize| package("word/document.xml", body, &[("word/_rels/document.xml.rels", rels.clone()), ("word/notes.bin", notes(n))]);
    let paragraphs = locos().with_limits(Limits::new().with_max_paragraphs(1000));
    assert_eq!(limit_kind(Docx.extract(&pkg(5000), &paragraphs)), LimitKind::Paragraphs);
    let elements = locos().with_limits(Limits::new().with_max_xml_elements(1000));
    assert_eq!(limit_kind(Docx.extract(&pkg(5000), &elements)), LimitKind::XmlElements);
    assert_eq!(Docx.extract(&pkg(5), &paragraphs).unwrap().units.len(), 1);
}
