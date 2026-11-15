//! Regression tests for the OpenDocument fixes of security review run 2: nested comments list each comment
//! paragraph once, naming new right-to-left sheet styles is linear, and a merge reads the stored archive under the
//! caller's merge limits.
mod common;
use bilingua_model::*;
use common::*;
use std::sync::mpsc;
use std::time::Duration;

/// Runs `f` on its own thread and waits at most `secs` seconds: a quadratic path fails the test at the deadline
/// instead of running for minutes.
fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(secs)).unwrap_or_else(|_| panic!("did not finish within {secs} s"))
}

fn content(body: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NSDECL}>{body}</office:document-content>")
}

fn comments_on() -> ExtractOptions {
    opts(None).with_format_option("odf.translate_comments", "true")
}

/// 200 comments nested in each other around 300 paragraphs: before the fix every paragraph was listed once per
/// enclosing comment (60,000 units, over the unit cap; at merge a list of nesting x paragraphs). Now each comment
/// paragraph is one unit, and a translated comment merges into its own paragraph.
#[test]
fn odf_nested_annotations_list_each_paragraph_once() {
    let paras: String = (0..300).map(|i| format!("<text:p>note {i}</text:p>")).collect();
    let body = format!(
        "<office:body><office:text><text:p>Body text</text:p>{}{paras}{}</office:text></office:body>",
        "<office:annotation>".repeat(200),
        "</office:annotation>".repeat(200)
    );
    let bytes = Pkg::new(content(&body)).build();
    let f = filter_for("odt");
    let mut doc = f.extract(&bytes, &comments_on()).unwrap_or_else(|e| panic!("extract: {e}"));
    let notes: Vec<&Unit> = doc.units.iter().filter(|u| u.id.starts_with('c')).collect();
    assert_eq!(notes.len(), 300);
    assert_eq!(doc.units.len(), 301);
    let u = doc.units.iter_mut().find(|u| u.id == "c7").unwrap();
    u.segments[0].target = vec![Inline::text("Anmerkung sieben")];
    u.segments[0].state = SegState::Translated;
    let out = f.merge(&doc).unwrap();
    let text = member_str(&out, "content.xml");
    assert!(text.contains("<text:p>Anmerkung sieben</text:p>"), "{}", &text[..text.len().min(400)]);
    assert!(!text.contains("note 7<"));
    assert!(text.contains("note 8<"));
}

/// A paragraph of a comment inside a tracked deletion is still left out, and a plain comment still gives its units.
#[test]
fn odf_comment_in_a_tracked_deletion_is_still_left_out() {
    let body = "<office:body><office:text><text:tracked-changes><text:changed-region><text:deletion><text:p>Gone<office:annotation><text:p>Hidden note</text:p></office:annotation></text:p></text:deletion></text:changed-region></text:tracked-changes><text:p>Kept<office:annotation><text:p>Seen note</text:p></office:annotation></text:p></office:text></office:body>";
    let doc = filter_for("odt").extract(&Pkg::new(content(body)).build(), &comments_on()).unwrap();
    let notes: Vec<String> = doc.units.iter().filter(|u| u.id.starts_with('c')).map(|u| plain_text(&u.source())).collect();
    assert_eq!(notes, ["Seen note"]);
}

/// 3,000 sheets that each name a different missing table style, in a file whose styles already take taRtl1 to
/// taRtl3000: merging to Arabic gives every sheet its own new style. Before the fix each new name was searched from
/// taRtl1 through a list (about 10^10 string compares); now it is linear.
#[test]
fn odf_rtl_many_sheets_style_names_linear() {
    let n = 3000;
    let taken: String = (1..=n).map(|i| format!("<style:style style:name=\"taRtl{i}\" style:family=\"paragraph\"/>")).collect();
    let tables: String = (0..n)
        .map(|i| format!("<table:table table:name=\"S{i}\" table:style-name=\"missing{i}\"><table:table-row><table:table-cell office:value-type=\"string\"><text:p>Cell {i}</text:p></table:table-cell></table:table-row></table:table>"))
        .collect();
    let body = format!("<office:automatic-styles>{taken}</office:automatic-styles><office:body><office:spreadsheet>{tables}</office:spreadsheet></office:body>");
    let bytes = Pkg::new(content(&body)).mime(MIME_SHEET).build();
    let out = within(20, move || {
        let f = filter_for("ods");
        let mut doc = f.extract(&bytes, &opts(Some("ar"))).unwrap();
        doc.units[0].segments[0].target = vec![Inline::text("خلية")];
        doc.units[0].segments[0].state = SegState::Translated;
        f.merge(&doc).unwrap()
    });
    let text = member_str(&out, "content.xml");
    assert!(text.contains(&format!("style:name=\"taRtl{}\"", n + 1)));
    assert!(text.contains(&format!("style:name=\"taRtl{}\"", 2 * n)));
    assert!(!text.contains(&format!("style:name=\"taRtl{}\"", 2 * n + 1)));
    assert!(text.contains("table:style-name=\"taRtl3001\""));
}

/// Two sheets naming the same missing style share one new style, and an existing table style is turned in place.
#[test]
fn odf_rtl_sheet_styles_are_shared_as_before() {
    let body = "<office:automatic-styles><style:style style:name=\"ta1\" style:family=\"table\"/><style:style style:name=\"taRtl1\" style:family=\"paragraph\"/></office:automatic-styles><office:body><office:spreadsheet><table:table table:style-name=\"ta1\"><table:table-row><table:table-cell office:value-type=\"string\"><text:p>One</text:p></table:table-cell></table:table-row></table:table><table:table table:style-name=\"gone\"><table:table-row><table:table-cell office:value-type=\"string\"><text:p>Two</text:p></table:table-cell></table:table-row></table:table><table:table table:style-name=\"gone\"/></office:spreadsheet></office:body>";
    let bytes = Pkg::new(content(body)).mime(MIME_SHEET).build();
    let out = build("ods", &bytes, &[], Some("ar"));
    let text = member_str(&out, "content.xml");
    assert_eq!(text.matches("table:style-name=\"taRtl2\"").count(), 2, "{text}");
    assert_eq!(text.matches("style:name=\"taRtl2\"").count(), 1);
    assert!(text.contains("rl-tb"));
}

/// The merge reads the stored archive under the caller's merge limits: a lowered element cap refuses the re-read,
/// the defaults merge it.
#[test]
fn office_merge_uses_caller_limits() {
    let paras: String = (0..500).map(|i| format!("<text:p>Line {i}</text:p>")).collect();
    let bytes = Pkg::new(content(&format!("<office:body><office:text>{paras}</office:text></office:body>"))).build();
    let f = filter_for("odt");
    let mut doc = f.extract(&bytes, &opts(None)).unwrap();
    doc.units[0].segments[0].target = vec![Inline::text("Zeile")];
    doc.units[0].segments[0].state = SegState::Translated;
    let tight = MergeOptions::new().with_limits(Limits::new().with_max_xml_elements(100));
    match f.merge_with(&doc, &tight) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::XmlElements),
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("the merge ignored the caller's element cap"),
    }
    let shallow = MergeOptions::new().with_limits(Limits::new().with_max_xml_depth(2));
    assert!(matches!(f.merge_with(&doc, &shallow), Err(Error::Limit(e)) if e.kind == LimitKind::XmlDepth));
    assert!(member_str(&f.merge(&doc).unwrap(), "content.xml").contains("<text:p>Zeile</text:p>"));
}
