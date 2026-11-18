//! bilingua#22: the context kind of an ODT unit is the one the DOCX reader gives the same content (`list-item`,
//! `header`, `footer`, `footnote`, `endnote`, `textbox`, ...), and under the LocOS profile it stays as before
//! (`heading`, `cell`, else `paragraph`), since the profile reads it for the LocOS style.
mod common;
use bilingua_model::*;
use bilingua_odf::*;
use common::*;

/// The issue's document: a heading, a paragraph, two list items, a 2 x 2 table, two paragraphs, a page header and a
/// page footer.
fn issue_odt() -> Vec<u8> {
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NSDECL}><office:body><office:text>\
<text:h text:outline-level=\"1\">Report</text:h><text:p>Intro words</text:p>\
<text:list><text:list-item><text:p>First item</text:p></text:list-item><text:list-item><text:p>Second item</text:p></text:list-item></text:list>\
<table:table><table:table-row><table:table-cell office:value-type=\"string\"><text:p>Cell a</text:p></table:table-cell>\
<table:table-cell office:value-type=\"string\"><text:p>Cell b</text:p></table:table-cell></table:table-row>\
<table:table-row><table:table-cell office:value-type=\"string\"><text:p>Cell c</text:p></table:table-cell>\
<table:table-cell office:value-type=\"string\"><text:p>Cell d</text:p></table:table-cell></table:table-row></table:table>\
<text:p>After the table</text:p><text:p>Last words</text:p></office:text></office:body></office:document-content>"
    );
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NSDECL}><office:master-styles>\
<style:master-page style:name=\"Standard\"><style:header><text:p>Header words</text:p></style:header>\
<style:footer><text:p>Footer words</text:p></style:footer></style:master-page></office:master-styles></office:document-styles>"
    );
    Pkg::new(content).styles(styles).build()
}

fn kinds(doc: &Document) -> Vec<&str> {
    doc.units.iter().map(|u| u.context.kind().unwrap_or("")).collect()
}

#[test]
fn odt_kinds_match_docx() {
    let data = issue_odt();
    let doc = extract("odt", &data, None);
    let cell = "cell";
    assert_eq!(
        kinds(&doc),
        ["heading", "paragraph", "list-item", "list-item", cell, cell, cell, cell, "paragraph", "paragraph", "header", "footer"],
        "the DOCX reader's sequence for the same document"
    );
    // roles and levels are as before
    assert_eq!((doc.units[0].context.role(), doc.units[0].context.level()), (Some(Role::Heading), Some(1)));
    assert_eq!(doc.units[2].context.role(), None);
    assert_eq!(doc.units[10].context.role(), Some(Role::Header));
    assert_eq!(doc.units[11].context.role(), Some(Role::Footer));
    // the kind is not read by the merge: an unchanged document gives the same bytes
    assert_eq!(Odt::default().merge(&doc).unwrap(), data);
}

#[test]
fn locos_profile_keeps_the_old_kinds() {
    let data = issue_odt();
    let doc = Odt::default().extract(&data, &opts(None).with_profile(profile::LOCOS)).unwrap();
    let (p, cell) = ("paragraph", "cell");
    assert_eq!(kinds(&doc), ["heading", p, p, p, cell, cell, cell, cell, p, p, p, p]);
    let suite = Odt::default().extract(&odt(), &opts(None).with_profile(profile::LOCOS)).unwrap();
    assert_eq!(kinds(&suite), ["heading", p, p, p, p, p, p, cell, p, p, p]);
}

#[test]
fn every_page_header_and_footer_variant() {
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NSDECL}><office:master-styles>\
<style:master-page style:name=\"Standard\"><style:header><text:h text:outline-level=\"1\">Head one</text:h></style:header>\
<style:header-left><text:p>Head left</text:p></style:header-left><style:header-first><text:p>Head first</text:p></style:header-first>\
<style:footer><table:table><table:table-row><table:table-cell><text:p>Foot cell</text:p></table:table-cell></table:table-row></table:table></style:footer>\
<style:footer-left><text:p>Foot left</text:p></style:footer-left><style:footer-first><text:p>Foot first</text:p></style:footer-first>\
</style:master-page></office:master-styles></office:document-styles>"
    );
    let data = Pkg::new(odt_content()).styles(styles).build();
    let doc = extract("odt", &data, None);
    let hf: Vec<&str> = doc.units.iter().filter(|u| &u.context.extra()["part"] == "styles.xml").map(|u| u.context.kind().unwrap()).collect();
    // as in DOCX, the part decides before the heading or the cell
    assert_eq!(hf, ["header", "header", "header", "footer", "footer", "footer"]);
}

#[test]
fn notes_text_boxes_and_lists_in_cells() {
    let content = odt_content().replace(
        "<text:p>12345</text:p>",
        concat!(
            r#"<text:p>Body<text:note text:id="en1" text:note-class="endnote"><text:note-citation>i</text:note-citation>"#,
            "<text:note-body><text:p>An endnote here.</text:p></text:note-body></text:note></text:p>",
            r#"<table:table><table:table-row><table:table-cell office:value-type="string"><text:list><text:list-item>"#,
            "<text:p>Listed in a cell</text:p></text:list-item></text:list></table:table-cell></table:table-row></table:table>",
            "<text:p>12345</text:p>"
        ),
    );
    let data = Pkg::new(content).styles(odt_styles()).build();
    let doc = extract("odt", &data, None);
    let kind = |text: &str| doc.units.iter().find(|u| plain_text(&u.source()) == text).unwrap_or_else(|| panic!("no unit {text}")).context.kind();
    assert_eq!(kind("A footnote about %1$s."), Some("footnote"));
    assert_eq!(kind("An endnote here."), Some("endnote"));
    assert_eq!(kind("Inside the box"), Some("textbox"));
    assert_eq!(kind("Caption"), Some("paragraph"));
    // DOCX: a numbered paragraph in a cell is a list item
    assert_eq!(kind("Listed in a cell"), Some("list-item"));
    assert_eq!(kind("Cell text"), Some("cell"));
}

#[test]
fn odp_speaker_notes_are_notes() {
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NSDECL}><office:body><office:presentation>\
<draw:page draw:name=\"p1\"><draw:frame><draw:text-box><text:p>Slide words</text:p></draw:text-box></draw:frame>\
<presentation:notes><draw:frame><draw:text-box><text:p>Say this slowly</text:p></draw:text-box></draw:frame></presentation:notes>\
</draw:page></office:presentation></office:body></office:document-content>"
    );
    let data = Pkg::new(content).mime(MIME_SLIDES).build();
    let doc = extract("odp", &data, None);
    // as PPTX for its notes slides; a slide's own text box keeps `paragraph` (every slide paragraph is in one)
    assert_eq!(kinds(&doc), ["paragraph", "notes"]);
    let ldoc = filter_for("odp").extract(&data, &opts(None).with_profile(profile::LOCOS)).unwrap();
    assert_eq!(kinds(&ldoc), ["paragraph", "paragraph"]);
}
