//! Structure roles (unit model step 4): headings with their level, page headers and footers, footnotes and
//! endnotes, comments, speaker notes; and none under the LocOS profile.
mod common;
use bilingua_model::*;
use bilingua_odf::*;
use common::*;

fn by_text<'a>(doc: &'a Document, text: &str) -> &'a Unit {
    doc.units.iter().find(|u| plain_text(&u.source()) == text).unwrap_or_else(|| panic!("no unit {text}"))
}

fn rl(doc: &Document, text: &str) -> (Option<Role>, Option<u8>) {
    let u = by_text(doc, text);
    (u.context.role(), u.context.level())
}

fn odt_with_notes() -> Vec<u8> {
    let content = odt_content().replace(
        "<text:p>12345</text:p>",
        concat!(
            r#"<text:h text:style-name="H2" text:outline-level="2">Second level</text:h>"#,
            "<text:h>No level given</text:h>",
            r#"<text:p>Body<text:note text:id="en1" text:note-class="endnote"><text:note-citation>i</text:note-citation>"#,
            "<text:note-body><text:p>An endnote here.</text:p></text:note-body></text:note></text:p>",
            "<text:p>12345</text:p>"
        ),
    );
    Pkg::new(content).styles(odt_styles()).build()
}

#[test]
fn odt_headings_headers_footers_notes_and_comments() {
    let data = odt_with_notes();
    let f = Odt::with_options({
        let mut o = OdfOptions::default();
        o.translate_comments = true;
        o
    });
    let doc = f.extract(&data, &opts(None)).unwrap();
    assert_eq!(rl(&doc, "Getting started"), (Some(Role::Heading), Some(1)));
    assert_eq!(rl(&doc, "Second level"), (Some(Role::Heading), Some(2)));
    assert_eq!(rl(&doc, "No level given"), (Some(Role::Heading), None));
    assert_eq!(rl(&doc, "A footnote about %1$s."), (Some(Role::Footnote), None));
    assert_eq!(rl(&doc, "An endnote here."), (Some(Role::Endnote), None));
    assert_eq!(rl(&doc, "Header line"), (Some(Role::Header), None));
    let footer = doc.units.iter().find(|u| plain_text(&u.source()).starts_with("Page")).expect("footer unit");
    assert_eq!((footer.context.role(), footer.context.level()), (Some(Role::Footer), None));
    assert_eq!(rl(&doc, "Comment text"), (Some(Role::Comment), None));
    assert_eq!(rl(&doc, "Cell text"), (None, None));
    assert_eq!(rl(&doc, "Inside the box"), (None, None));
    // what the crate filled before stays: kind, the nearest heading, p_index
    let h = by_text(&doc, "Second level");
    assert_eq!(h.context.kind(), Some("heading"));
    assert_eq!(h.context.heading(), Some("Getting started"));
    assert!(h.context.extra().get("p_index").is_some());
    assert_eq!(by_text(&doc, "Comment text").context.kind(), Some("comment"));
    // an unchanged document still merges to the same archive
    assert_eq!(f.merge(&doc).unwrap(), data);

    // the LocOS profile leaves roles unset, also through the facade-style annotate
    let lopts = opts(None).with_profile(profile::LOCOS);
    let ldoc = f.extract(&data, &lopts).unwrap();
    assert!(!ldoc.units.is_empty());
    assert!(ldoc.units.iter().all(|u| u.context.role().is_none() && u.context.level().is_none()));
}

#[test]
fn odp_speaker_notes() {
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NSDECL}><office:body><office:presentation>\
<draw:page draw:name=\"p1\"><draw:frame><draw:text-box><text:p>Slide words</text:p></draw:text-box></draw:frame>\
<presentation:notes><draw:frame><draw:text-box><text:p>Say this slowly</text:p></draw:text-box></draw:frame></presentation:notes>\
</draw:page></office:presentation></office:body></office:document-content>"
    );
    let data = Pkg::new(content).mime(MIME_SLIDES).build();
    let doc = extract("odp", &data, None);
    assert_eq!(rl(&doc, "Say this slowly"), (Some(Role::Notes), None));
    assert_eq!(rl(&doc, "Slide words"), (None, None));
    let ldoc = filter_for("odp").extract(&data, &opts(None).with_profile(profile::LOCOS)).unwrap();
    assert!(ldoc.units.iter().all(|u| u.context.role().is_none()));
}
