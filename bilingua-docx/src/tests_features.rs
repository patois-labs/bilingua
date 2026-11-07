use crate::tests::*;
use crate::*;
use std::io::Write;

fn seg_of<'a>(doc: &'a Document, id: &str) -> &'a Segment {
    doc.find(id).unwrap().1
}

fn tag_displays(s: &Segment) -> Vec<String> {
    s.source
        .iter()
        .filter_map(|i| {
            if let Inline::Tag { display, .. } = i {
                Some(display.clone())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn split_runs_are_merged() {
    let inner = "<w:p><w:r><w:rPr><w:lang w:val=\"en-US\"/></w:rPr><w:t>Hel</w:t></w:r>\
<w:proofErr w:type=\"spellStart\"/><w:r w:rsidR=\"00A1B2C3\"><w:rPr><w:noProof/></w:rPr><w:t>lo wor</w:t></w:r>\
<w:proofErr w:type=\"spellEnd\"/><w:r><w:t>ld</w:t></w:r></w:p>";
    let bytes = doc_of(inner);
    let mut doc = round_trip(&bytes);
    let s = seg_of(&doc, "p1");
    assert_eq!(s.source, vec![Inline::text("Hello world")]);
    doc.target_lang = Some("es-ES".into());
    doc.set_target("p1", vec![Inline::text("Hola mundo")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:t>Hola mundo</w:t>"));
    assert!(!x.contains("proofErr"));
}

#[test]
fn bold_and_italic_become_paired_tags() {
    let inner = "<w:p><w:r><w:t xml:space=\"preserve\">Press </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>Save</w:t></w:r>\
<w:r><w:t xml:space=\"preserve\"> to </w:t></w:r><w:r><w:rPr><w:i/><w:color w:val=\"FF0000\"/></w:rPr><w:t>keep</w:t></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    let s = seg_of(&doc, "p1");
    assert_eq!(tag_displays(s), vec!["<b>", "</b>", "<fmt>", "</fmt>"]);
    assert_eq!(plain_text(&s.source), "Press Save to keep");
    let u = &doc.units[0];
    assert_eq!(u.codes["1"].label.as_deref(), Some("bold"));
    assert_eq!(u.codes["2"].label.as_deref(), Some("italic color"));
    // Translator reorders: the italic part first.
    let t = vec![
        Inline::tag("2", TagKind::Open, "<fmt>"),
        Inline::text("conservar"),
        Inline::tag("2", TagKind::Close, "</fmt>"),
        Inline::text(" con "),
        Inline::tag("1", TagKind::Open, "<b>"),
        Inline::text("Guardar"),
        Inline::tag("1", TagKind::Close, "</b>"),
    ];
    doc.set_target("p1", t).unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains(
        "<w:r><w:rPr><w:i/><w:color w:val=\"FF0000\"/></w:rPr><w:t>conservar</w:t></w:r>"
    ));
    assert!(x.contains("<w:r><w:rPr><w:b/></w:rPr><w:t>Guardar</w:t></w:r>"));
    assert!(x.contains("<w:r><w:t xml:space=\"preserve\"> con </w:t></w:r>"));
    // Tags the source lacks are refused by the model.
    assert!(doc
        .set_target("p1", vec![Inline::tag("9", TagKind::Open, "<b>")])
        .is_err());
}

#[test]
fn whole_paragraph_in_one_format_has_no_tags() {
    let inner = "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>All bold </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>text</w:t></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    assert!(tag_displays(seg_of(&doc, "p1")).is_empty());
    doc.set_target("p1", vec![Inline::text("Todo en negrita")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:r><w:rPr><w:b/></w:rPr><w:t>Todo en negrita</w:t></w:r>"));
}

const STYLES: &str = "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:docDefaults><w:rPrDefault><w:rPr><w:lang w:val=\"en-GB\"/></w:rPr></w:rPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Titel\"><w:name w:val=\"Title\"/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Bullets\"><w:name w:val=\"Bullets\"/><w:pPr><w:numPr><w:numId w:val=\"3\"/></w:numPr></w:pPr></w:style>\
</w:styles>";

#[test]
fn headings_lists_tables_have_context() {
    let inner = "<w:p><w:pPr><w:pStyle w:val=\"Titel\"/></w:pPr><w:r><w:t>Annual report</w:t></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Overview</w:t></w:r></w:p>\
<w:p><w:r><w:t>Body text.</w:t></w:r></w:p>\
<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:r><w:t>First point</w:t></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Bullets\"/></w:pPr><w:r><w:t>Styled bullet</w:t></w:r></w:p>\
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cell B</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let bytes = make(&[
        ("word/document.xml", &body(inner)),
        ("word/styles.xml", STYLES),
    ]);
    let doc = round_trip(&bytes);
    assert_eq!(doc.source_lang.as_deref(), Some("en-GB"));
    let ctx = |id: &str| {
        doc.units
            .iter()
            .find(|u| u.id == id)
            .unwrap()
            .context
            .clone()
    };
    assert_eq!(ctx("p1").kind.as_deref(), Some("heading"));
    assert_eq!(ctx("p1").style.as_deref(), Some("Titel"));
    assert_eq!(ctx("p2").kind.as_deref(), Some("heading"));
    assert_eq!(ctx("p2").heading.as_deref(), Some("Annual report"));
    assert_eq!(ctx("p3").kind.as_deref(), Some("paragraph"));
    assert_eq!(ctx("p3").heading.as_deref(), Some("Overview"));
    assert_eq!(ctx("p4").kind.as_deref(), Some("list-item"));
    assert_eq!(ctx("p5").kind.as_deref(), Some("list-item"));
    assert_eq!(ctx("p6").kind.as_deref(), Some("cell"));
    assert_eq!(ctx("p7").kind.as_deref(), Some("cell"));
    assert_eq!(
        ctx("p7").location.as_deref(),
        Some("word/document.xml paragraph 7")
    );
}

#[test]
fn other_parts_headers_footnotes_comments_properties() {
    let docxml = "<w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"2\"/></w:r></w:p>\
<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t>Commented words</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>";
    let header = part("hdr", &p("Acme Corp confidential"));
    let footer = part("ftr", "<w:p><w:r><w:t xml:space=\"preserve\">Page </w:t></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>");
    let notes = part("footnotes", "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>\
<w:footnote w:id=\"2\"><w:p><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> See the appendix.</w:t></w:r></w:p></w:footnote>");
    let comments = part("comments", "<w:comment w:id=\"0\" w:author=\"Dana\" w:date=\"2026-01-01T00:00:00Z\"><w:p><w:r><w:t>Keep the brand name</w:t></w:r></w:p></w:comment>");
    let core = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Quarterly &amp; yearly plan</dc:title><dc:subject>Planning</dc:subject><dc:creator>Dana</dc:creator></cp:coreProperties>";
    let bytes = make(&[
        ("word/document.xml", &body(docxml)),
        ("word/header1.xml", &header),
        ("word/footer1.xml", &footer),
        ("word/footnotes.xml", &notes),
        ("word/comments.xml", &comments),
        ("docProps/core.xml", core),
    ]);
    let mut doc = round_trip(&bytes);
    let ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "p1",
            "p2",
            "header1-p1",
            "footer1-p1",
            "footnotes-p2",
            "comments-p1",
            "meta-title",
            "meta-subject"
        ]
    );
    let kind = |id: &str| {
        doc.units
            .iter()
            .find(|u| u.id == id)
            .unwrap()
            .context
            .kind
            .clone()
            .unwrap()
    };
    assert_eq!(kind("header1-p1"), "header");
    assert_eq!(kind("footer1-p1"), "footer");
    assert_eq!(kind("footnotes-p2"), "footnote");
    assert_eq!(kind("comments-p1"), "comment");
    assert_eq!(kind("meta-title"), "title");
    // Footnote reference stays in the sentence as a tag, the page number field is one locked tag.
    assert_eq!(tag_displays(seg_of(&doc, "p1")), vec!["[fn 2]"]);
    assert_eq!(tag_displays(seg_of(&doc, "footer1-p1")), vec!["{PAGE 1}"]);
    assert_eq!(src(&doc, "footnotes-p2"), " See the appendix.");
    assert_eq!(src(&doc, "meta-title"), "Quarterly & yearly plan");
    // The comment is a note of the paragraph it anchors to.
    let u = doc.units.iter().find(|u| u.id == "p2").unwrap();
    assert_eq!(
        u.context.notes,
        vec!["Comment (Dana): Keep the brand name".to_string()]
    );

    doc.set_target(
        "meta-title",
        vec![Inline::text("Plan <trimestral> & anual")],
    )
    .unwrap();
    doc.set_target("header1-p1", vec![Inline::text("Acme Corp confidencial")])
        .unwrap();
    doc.set_target("footnotes-p2", vec![Inline::text(" Véase el apéndice.")])
        .unwrap();
    doc.set_target("comments-p1", vec![Inline::text("Mantener la marca")])
        .unwrap();
    let out = Docx.merge(&doc).unwrap();
    assert!(xml_of(&out, "docProps/core.xml")
        .contains("<dc:title>Plan &lt;trimestral&gt; &amp; anual</dc:title>"));
    assert!(xml_of(&out, "word/header1.xml").contains("Acme Corp confidencial"));
    assert!(xml_of(&out, "word/footnotes.xml")
        .contains("<w:t xml:space=\"preserve\"> Véase el apéndice.</w:t>"));
    assert!(xml_of(&out, "word/footnotes.xml").contains("<w:footnoteRef/>"));
    assert!(xml_of(&out, "word/footnotes.xml").contains("w:type=\"separator\""));
    // The footer is untouched, so it is byte for byte the same.
    assert_eq!(xml_of(&out, "word/footer1.xml"), footer);
    // Comment anchors survive in the rebuilt paragraph.
    doc.set_target("p2", vec![Inline::text("Palabras comentadas")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(
        x.contains("<w:commentRangeStart w:id=\"0\"/><w:r><w:t>Palabras comentadas</w:t></w:r>")
    );
    assert!(x.contains(
        "<w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>"
    ));
}

#[test]
fn hyperlink_is_a_paired_tag_with_its_relationship() {
    let inner = "<w:p><w:r><w:t xml:space=\"preserve\">Visit </w:t></w:r><w:hyperlink r:id=\"rId7\" w:history=\"1\">\
<w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr><w:t>our site</w:t></w:r></w:hyperlink><w:r><w:t>.</w:t></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    let s = seg_of(&doc, "p1");
    assert_eq!(tag_displays(s), vec!["<a>", "</a>"]);
    doc.set_target(
        "p1",
        vec![
            Inline::text("Visite "),
            Inline::tag("1", TagKind::Open, "<a>"),
            Inline::text("nuestro sitio"),
            Inline::tag("1", TagKind::Close, "</a>"),
            Inline::text("."),
        ],
    )
    .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains(
        "<w:hyperlink r:id=\"rId7\" w:history=\"1\"><w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr><w:t>nuestro sitio</w:t></w:r></w:hyperlink>"
    ));
}

#[test]
fn toc_field_spanning_paragraphs_is_locked() {
    let inner = "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> TOC \\o \"1-3\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>Intro 1</w:t></w:r></w:p>\
<w:p><w:r><w:t>Methods 4</w:t></w:r></w:p>\
<w:p><w:r><w:t>Results 7</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>\
<w:p><w:r><w:t>Real text after the table of contents.</w:t></w:r></w:p>";
    let doc = round_trip(&doc_of(inner));
    assert_eq!(doc.units.len(), 1);
    assert_eq!(src(&doc, "p4"), "Real text after the table of contents.");
}

#[test]
fn simple_field_and_text_around_a_complex_field() {
    let inner = "<w:p><w:r><w:t xml:space=\"preserve\">Page </w:t></w:r><w:fldSimple w:instr=\" PAGE \"><w:r><w:t>3</w:t></w:r></w:fldSimple><w:r><w:t xml:space=\"preserve\"> of </w:t></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>12</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    let s = seg_of(&doc, "p1");
    assert_eq!(tag_displays(s), vec!["{PAGE 3}", "{NUMPAGES 12}"]);
    doc.set_target(
        "p1",
        vec![
            Inline::text("Página "),
            Inline::tag("1", TagKind::Standalone, ""),
            Inline::text(" de "),
            Inline::tag("2", TagKind::Standalone, ""),
        ],
    )
    .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:fldSimple w:instr=\" PAGE \"><w:r><w:t>3</w:t></w:r></w:fldSimple>"));
    assert!(x.contains("<w:instrText> NUMPAGES </w:instrText>"));
}

#[test]
fn tabs_breaks_and_images_are_standalone_tags() {
    let inner = "<w:p><w:r><w:t>Name</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>Value</w:t><w:br/><w:t>Next line</w:t></w:r>\
<w:r><w:drawing><wp:inline><a:graphic/></wp:inline></w:drawing></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    let s = seg_of(&doc, "p1");
    assert_eq!(tag_displays(s), vec!["<tab>", "<br>", "<img>"]);
    let t: Vec<Inline> = s
        .source
        .iter()
        .map(|i| match i {
            Inline::Text(t) => Inline::text(t.to_uppercase()),
            o => o.clone(),
        })
        .collect();
    doc.set_target("p1", t).unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:t>NAME</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>VALUE</w:t></w:r><w:r><w:br/></w:r><w:r><w:t>NEXT LINE</w:t></w:r><w:r><w:drawing>"));
}

#[test]
fn text_box_with_fallback_is_translated_in_both_copies() {
    let tb =
        |t: &str| format!("<w:txbxContent><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:txbxContent>");
    let inner = format!(
        "<w:p><w:r><w:t>Before</w:t></w:r><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wps:wsp><wps:txbx>{}</wps:txbx></wps:wsp></w:drawing></mc:Choice>\
<mc:Fallback><w:pict><v:shape><v:textbox>{}</v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>",
        tb("Sale ends Friday"),
        tb("Sale ends Friday")
    );
    let mut doc = round_trip(&doc_of(&inner));
    let ids: Vec<String> = doc.units.iter().map(|u| u.id.clone()).collect();
    assert_eq!(ids, vec!["p1", "p2"]);
    assert_eq!(doc.units[1].context.kind.as_deref(), Some("textbox"));
    assert_eq!(doc.units[0].segments[0].source.len(), 2); // "Before" and the textbox tag
    doc.set_target("p2", vec![Inline::text("Oferta hasta el viernes")])
        .unwrap();
    doc.set_target(
        "p1",
        vec![
            Inline::text("Antes"),
            Inline::tag("1", TagKind::Standalone, "<textbox>"),
        ],
    )
    .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert_eq!(
        x.matches("Oferta hasta el viernes").count(),
        2,
        "choice and fallback must both change"
    );
    assert!(!x.contains("Sale ends"));
    assert!(x.contains("Antes"));
}

#[test]
fn unmatched_fallback_is_left_alone() {
    let inner = "<w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wps:txbx><w:txbxContent><w:p><w:r><w:t>New text</w:t></w:r></w:p></w:txbxContent></wps:txbx></w:drawing></mc:Choice>\
<mc:Fallback><w:pict><v:textbox><w:txbxContent><w:p><w:r><w:t>Old text</w:t></w:r></w:p></w:txbxContent></v:textbox></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    assert_eq!(doc.units.len(), 1);
    doc.set_target("p2", vec![Inline::text("Texto nuevo")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("Texto nuevo") && x.contains("Old text"));
}

#[test]
fn vml_text_box_without_alternate_content() {
    let inner = "<w:p><w:r><w:pict><v:shape><v:textbox><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    assert_eq!(doc.units.len(), 1);
    doc.set_target("p2", vec![Inline::text("Encuadrado")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:t>Encuadrado</w:t>"));
}

#[test]
fn tracked_changes_are_accepted_for_text_and_kept_when_unchanged() {
    let inner = "<w:p><w:r><w:t xml:space=\"preserve\">The price is </w:t></w:r>\
<w:del w:id=\"1\" w:author=\"A\" w:date=\"2026-01-01T00:00:00Z\"><w:r><w:delText>ten</w:delText></w:r></w:del>\
<w:ins w:id=\"2\" w:author=\"A\" w:date=\"2026-01-01T00:00:00Z\"><w:r><w:t>twelve</w:t></w:r></w:ins><w:r><w:t> dollars.</w:t></w:r></w:p>";
    let bytes = doc_of(inner);
    let mut doc = round_trip(&bytes); // unchanged: identical, tracked markup kept
    assert_eq!(src(&doc, "p1"), "The price is twelve dollars.");
    assert_eq!(
        doc.units[0].context.notes,
        vec!["This paragraph has tracked changes".to_string()]
    );
    doc.set_target("p1", vec![Inline::text("El precio es doce dólares.")])
        .unwrap();
    let (out, rep) = merge_with_report(&doc).unwrap();
    let x = xml_of(&out, "word/document.xml");
    assert!(!x.contains("w:del") && !x.contains("w:ins"));
    assert!(x.contains("El precio es doce dólares."));
    assert_eq!(rep.warnings.len(), 1);
    assert!(rep.warnings[0].contains("p1") && rep.warnings[0].contains("tracked changes"));
}

#[test]
fn arabic_rtl_document() {
    let ar = "مرحبا بكم في شركتنا";
    let inner = format!(
        "<w:p><w:pPr><w:bidi/></w:pPr><w:r><w:rPr><w:rtl/><w:lang w:bidi=\"ar-SA\"/></w:rPr><w:t>{ar}</w:t></w:r></w:p>\
<w:p><w:pPr><w:bidi/></w:pPr><w:r><w:rPr><w:b/><w:rtl/></w:rPr><w:t>مهم</w:t></w:r><w:r><w:rPr><w:rtl/></w:rPr><w:t xml:space=\"preserve\"> جدا</w:t></w:r></w:p>"
    );
    let mut doc = round_trip(&doc_of(&inner));
    assert_eq!(src(&doc, "p1"), ar);
    assert_eq!(tag_displays(seg_of(&doc, "p2")), vec!["<b>", "</b>"]);
    doc.target_lang = Some("ar-EG".into());
    doc.set_target("p1", vec![Inline::text("أهلا وسهلا")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:pPr><w:bidi/></w:pPr><w:r><w:rPr><w:rtl/><w:lang w:bidi=\"ar-EG\"/></w:rPr><w:t>أهلا وسهلا</w:t></w:r>"));
    let html = Docx.preview(&doc).unwrap();
    assert!(html.contains("<p dir=\"rtl\">"));
}

#[test]
fn japanese_document_language_marks() {
    let ja = "ご注文ありがとうございます。";
    let inner = format!("<w:p><w:r><w:rPr><w:lang w:val=\"en-US\" w:eastAsia=\"ja-JP\"/></w:rPr><w:t>{ja}</w:t></w:r></w:p>");
    let mut doc = round_trip(&doc_of(&inner));
    assert_eq!(src(&doc, "p1"), ja);
    doc.target_lang = Some("es-ES".into());
    doc.set_target("p1", vec![Inline::text("Gracias por su pedido.")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    // The Latin language follows the target, the East Asian mark stays (it describes other text).
    assert!(x.contains("<w:lang w:val=\"es-ES\" w:eastAsia=\"ja-JP\"/>"));
    // Translating into Japanese sets the East Asian mark too.
    doc.target_lang = Some("ja-JP".into());
    doc.set_target("p1", vec![Inline::text("ありがとう。")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:lang w:val=\"ja-JP\" w:eastAsia=\"ja-JP\"/>"));
}

#[test]
fn writing_escapes_and_preserves_space() {
    let bytes = doc_of(&p("x"));
    let mut doc = extract(&bytes);
    doc.set_target(
        "p1",
        vec![Inline::text("  a < b && c > d\tend\nnext \u{1} ")],
    )
    .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:t xml:space=\"preserve\">  a &lt; b &amp;&amp; c &gt; d</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>end</w:t></w:r><w:r><w:br/></w:r><w:r><w:t xml:space=\"preserve\">next  </w:t></w:r>"), "{x}");
    assert!(crate::xml::parse(&x).is_ok());
}

#[test]
fn zip_container_details_survive() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt = zip::write::SimpleFileOptions::default();
    w.start_file(
        "[Content_Types].xml",
        opt.compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    w.write_all(CT.as_bytes()).unwrap();
    w.start_file(
        "word/media/data.bin",
        opt.compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    w.write_all(&[0, 1, 2, 3, 255]).unwrap();
    w.start_file(
        "word/document.xml",
        opt.compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    w.write_all(body(&p("Hi")).as_bytes()).unwrap();
    w.start_file(
        "customXml/z.xml",
        opt.compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    w.write_all(b"<z/>").unwrap();
    let bytes = w.finish().unwrap().into_inner();
    let mut doc = extract(&bytes);
    doc.set_target("p1", vec![Inline::text("Hola")]).unwrap();
    let out = Docx.merge(&doc).unwrap();
    let names: Vec<String> = entries_of(&out).into_iter().map(|e| e.0).collect();
    assert_eq!(
        names,
        vec![
            "[Content_Types].xml",
            "word/media/data.bin",
            "word/document.xml",
            "customXml/z.xml"
        ]
    );
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&out)).unwrap();
    assert_eq!(
        z.by_name("word/media/data.bin").unwrap().compression(),
        zip::CompressionMethod::Stored
    );
    assert_eq!(
        z.by_name("word/document.xml").unwrap().compression(),
        zip::CompressionMethod::Deflated
    );
    assert!(!names.iter().any(|n| n.contains("bilingua")));
}

fn raw_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (n, d) in entries {
        w.start_file(*n, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(d).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[test]
fn hostile_input_is_refused() {
    let ok = body(&p("x"));
    let f = Docx;
    let o = ExtractOptions::default();
    assert!(f
        .extract(b"not a zip at all, just some text bytes", &o)
        .is_err());
    assert!(f.extract(&raw_zip(&[("a.txt", b"x")]), &o).is_err()); // no content types
    let bad_names = ["../evil.xml", "/abs.xml", "word/../../x.xml", "C:evil.xml"];
    for n in bad_names {
        let z = raw_zip(&[
            ("[Content_Types].xml", CT.as_bytes()),
            ("word/document.xml", ok.as_bytes()),
            (n, b"x"),
        ]);
        assert!(f.extract(&z, &o).is_err(), "{n} must be refused");
    }
    let dup = raw_zip(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("word/document.xml", ok.as_bytes()),
        ("WORD/document.xml", b"x"),
    ]);
    assert!(f.extract(&dup, &o).is_err());
    // Expansion ratio: 8 MB of zeros packs to a few KB.
    let zeros = vec![0u8; 8 * 1024 * 1024];
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let d = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    w.start_file("[Content_Types].xml", d).unwrap();
    w.write_all(CT.as_bytes()).unwrap();
    w.start_file("word/document.xml", d).unwrap();
    w.write_all(ok.as_bytes()).unwrap();
    w.start_file("word/bomb.bin", d).unwrap();
    w.write_all(&zeros).unwrap();
    let bomb = w.finish().unwrap().into_inner();
    let err = f.extract(&bomb, &o).unwrap_err().to_string();
    assert!(err.contains("expands"), "{err}");
    // DOCTYPE with an entity is refused, not expanded.
    let evil = format!("<?xml version=\"1.0\"?><!DOCTYPE d [<!ENTITY x \"boom\">]><w:document {NS}><w:body><w:p><w:r><w:t>&x;</w:t></w:r></w:p></w:body></w:document>");
    assert!(f
        .extract(&make(&[("word/document.xml", &evil)]), &o)
        .is_err());
    // Broken XML in the main part.
    assert!(f
        .extract(
            &make(&[("word/document.xml", "<w:document><w:body></w:document>")]),
            &o
        )
        .is_err());
}

#[test]
fn broken_side_parts_are_left_untouched() {
    let hdr = "<w:hdr><w:p>"; // not well formed
    let bytes = make(&[
        ("word/document.xml", &body(&p("Hi"))),
        ("word/header1.xml", hdr),
    ]);
    let doc = round_trip(&bytes);
    assert_eq!(doc.units.len(), 1);
}

#[test]
fn sniff_and_extensions() {
    let bytes = doc_of(&p("x"));
    assert!(Docx.sniff(&bytes));
    assert!(!Docx.sniff(b"plain text"));
    assert!(!Docx.sniff(&raw_zip(&[("a.txt", b"x")])));
    assert!(Docx.extensions().contains(&"docx"));
}

#[test]
fn other_prefixes_and_default_namespace() {
    let doc_xml = "<?xml version=\"1.0\"?><x:document xmlns:x=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><x:body><x:p><x:r><x:rPr><x:b/></x:rPr><x:t>Pre</x:t></x:r><x:r><x:t>fix test</x:t></x:r></x:p></x:body></x:document>";
    let mut doc = round_trip(&make(&[("word/document.xml", doc_xml)]));
    assert_eq!(tag_displays(seg_of(&doc, "p1")), vec!["<b>", "</b>"]);
    doc.set_target(
        "p1",
        vec![
            Inline::tag("1", TagKind::Open, ""),
            Inline::text("Prue"),
            Inline::tag("1", TagKind::Close, ""),
            Inline::text("ba de prefijo"),
        ],
    )
    .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<x:r><x:rPr><x:b/></x:rPr><x:t>Prue</x:t></x:r>"));
    assert!(x.contains("<x:r><x:t>ba de prefijo</x:t></x:r>"));
    let dflt = "<document xmlns=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><body><p><r><t>Default ns</t></r></p></body></document>";
    let d2 = round_trip(&make(&[("word/document.xml", dflt)]));
    assert_eq!(src(&d2, "p1"), "Default ns");
}

#[test]
fn content_control_wraps_runs() {
    let inner = "<w:p><w:sdt><w:sdtPr><w:alias w:val=\"Name\"/></w:sdtPr><w:sdtContent><w:r><w:t>Customer name</w:t></w:r></w:sdtContent></w:sdt><w:r><w:t> here</w:t></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    assert_eq!(tag_displays(seg_of(&doc, "p1")), vec!["<x>", "</x>"]);
    doc.set_target(
        "p1",
        vec![
            Inline::tag("1", TagKind::Open, ""),
            Inline::text("Nombre del cliente"),
            Inline::tag("1", TagKind::Close, ""),
            Inline::text(" aquí"),
        ],
    )
    .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:sdt><w:sdtPr><w:alias w:val=\"Name\"/></w:sdtPr><w:sdtContent><w:r><w:t>Nombre del cliente</w:t></w:r></w:sdtContent></w:sdt>"));
}

#[test]
fn empty_and_whitespace_paragraphs_are_not_units() {
    let inner = "<w:p/><w:p><w:r><w:t xml:space=\"preserve\">   </w:t></w:r></w:p><w:p><w:r><w:tab/></w:r></w:p><w:p><w:r><w:t>Real</w:t></w:r></w:p>";
    let doc = round_trip(&doc_of(inner));
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].id, "p4");
}

#[test]
fn preview_has_structure_and_segment_spans() {
    let inner = "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Welcome</w:t></w:r></w:p>\
<w:p><w:r><w:t xml:space=\"preserve\">Some </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t xml:space=\"preserve\"> &amp; </w:t></w:r><w:r><w:rPr><w:u w:val=\"single\"/></w:rPr><w:t>u&lt;</w:t></w:r><w:r><w:drawing/></w:r></w:p>\
<w:p><w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:r><w:t>Item one</w:t></w:r></w:p>\
<w:p><w:pPr><w:numPr><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:r><w:t>Item two</w:t></w:r></w:p>\
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let hdr = part("hdr", &p("Top of page"));
    let bytes = make(&[
        ("word/document.xml", &body(inner)),
        ("word/styles.xml", STYLES),
        ("word/header1.xml", &hdr),
    ]);
    let mut doc = extract(&bytes);
    doc.set_target("p2", vec![Inline::text("Un texto <i>")])
        .unwrap();
    let html = Docx.preview(&doc).unwrap();
    assert!(
        html.contains("<h1><span data-bl-seg=\"p1\">Welcome</span></h1>"),
        "{html}"
    );
    assert!(html.contains("<span data-bl-seg=\"p2\">Un texto &lt;i&gt;</span>"));
    assert!(html.contains("<ul><li><span data-bl-seg=\"p3\">Item one</span></li><li><span data-bl-seg=\"p4\">Item two</span></li></ul>"));
    assert!(html
        .contains("<table><tr><td><p><span data-bl-seg=\"p5\">Cell</span></p></td></tr></table>"));
    assert!(html.contains("data-bl-seg=\"header1-p1\""));
    assert!(!html.contains("<script"));
    doc.set_target(
        "p2",
        vec![
            Inline::text("Some "),
            Inline::tag("1", TagKind::Open, ""),
            Inline::text("bold"),
            Inline::tag("1", TagKind::Close, ""),
            Inline::text(" & "),
            Inline::tag("2", TagKind::Open, ""),
            Inline::text("u"),
            Inline::tag("2", TagKind::Close, ""),
            Inline::tag("3", TagKind::Standalone, ""),
        ],
    )
    .unwrap();
    assert_eq!(
        Docx.render_segment(&doc, "p2").unwrap(),
        "Some <b>bold</b> &amp; <u>u</u><span class=\"bl-img\">[image]</span>"
    );
    assert!(Docx.render_segment(&doc, "nope").is_none());
}

#[test]
fn units_survive_segment_splitting_by_the_model() {
    // A later stage may split a unit in sentences; the writer joins them with the separators.
    let bytes = doc_of(&p("One. Two."));
    let mut doc = extract(&bytes);
    let u = &mut doc.units[0];
    u.segments = vec![
        Segment::new("p1#1", vec![Inline::text("One.")]).with_target(vec![Inline::text("Uno.")]).with_state(SegState::Draft),
        Segment::new("p1#2", vec![Inline::text("Two.")]).with_target(vec![Inline::text("Dos.")]).with_state(SegState::Draft),
    ];
    u.separators = vec![" ".into()];
    assert_eq!(plain_text(&u.source()), "One. Two.");
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:t>Uno. Dos.</w:t>"));
    // Untouched split document is still byte-identical.
    for s in &mut doc.units[0].segments {
        s.target.clear();
    }
    let (out, _) = merge_with_report(&doc).unwrap();
    same_entries(&bytes, &out);
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
    let mut inner: String = LANGS.iter().map(|(_, t)| p(t)).collect();
    inner.push_str(&rich());
    let mut doc = extract(&doc_of(&inner));
    doc.set_target("p1", vec![Inline::text("Hola.")]).unwrap();
    for (i, u) in doc.units.clone().iter().enumerate().skip(11) {
        let t: Vec<Inline> = u
            .source()
            .iter()
            .map(|x| match x {
                Inline::Text(t) => Inline::text(format!("[{i}] {t}")),
                o => o.clone(),
            })
            .collect();
        doc.set_target(&u.id, t).unwrap();
    }
    let out = Docx.merge(&doc).unwrap();
    let dir = std::env::temp_dir().join(format!("bilingua-docx-lo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("merged.docx");
    std::fs::write(&file, out).unwrap();
    let status = std::process::Command::new(soffice)
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(&dir)
        .arg(&file)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(
        dir.join("merged.pdf").exists(),
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn rich() -> String {
    let tb =
        |t: &str| format!("<w:txbxContent><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:txbxContent>");
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Some </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t xml:space=\"preserve\"> and more text here</w:t></w:r></w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>Cell text</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:inline><wp:extent cx=\"1000000\" cy=\"500000\"/><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:txbx>{}</wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape style=\"width:100pt;height:40pt\"><v:textbox>{}</v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>",
        tb("Boxed words"),
        tb("Boxed words")
    )
}

#[test]
fn damaged_documents_never_panic() {
    let mut inner: String = LANGS.iter().map(|(_, t)| p(t)).collect();
    inner.push_str(&rich());
    let full = body(&inner);
    let mut tried = 0;
    for cut in (0..full.len()).step_by(37) {
        if !full.is_char_boundary(cut) {
            continue;
        }
        let bytes = make(&[("word/document.xml", &full[..cut])]);
        let _ = Docx.extract(&bytes, &ExtractOptions::default());
        tried += 1;
    }
    assert!(tried > 20);
    // Garbled bytes inside the XML (still valid UTF-8).
    for k in (0..full.len()).step_by(53) {
        if !full.is_char_boundary(k) || !full.is_char_boundary(k + 1) {
            continue;
        }
        let mut s = full.clone();
        s.replace_range(k..k + 1, "<");
        let bytes = make(&[("word/document.xml", &s)]);
        if let Ok(doc) = Docx.extract(&bytes, &ExtractOptions::default()) {
            let _ = Docx.merge(&doc);
        }
    }
}

#[test]
fn rich_document_round_trips_and_extracts_the_textbox_once() {
    let mut inner = p("Intro");
    inner.push_str(&rich());
    let doc = round_trip(&doc_of(&inner));
    let texts: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(
        texts,
        [
            "Intro",
            "Some bold and more text here",
            "Cell text",
            "Boxed words"
        ]
    );
}

#[test]
fn text_box_in_a_paragraph_without_text_changes_in_both_copies() {
    let mut doc = extract(&doc_of(&rich()));
    let id = doc
        .units
        .iter()
        .find(|u| plain_text(&u.source()) == "Boxed words")
        .unwrap()
        .id
        .clone();
    doc.set_target(&id, vec![Inline::text("Palabras en caja")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert_eq!(x.matches("Palabras en caja").count(), 2);
    assert!(!x.contains("Boxed words"));
    assert!(!x.contains('\u{E003}') && !x.contains('\u{E000}'));
}

#[test]
fn a_long_document_is_fast_enough() {
    let inner: String = (0..5000)
        .map(|i| format!("<w:p><w:r><w:t xml:space=\"preserve\">Paragraph {i} with </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t> words.</w:t></w:r></w:p>"))
        .collect();
    let bytes = doc_of(&inner);
    let t = std::time::Instant::now();
    let doc = round_trip(&bytes);
    assert_eq!(doc.units.len(), 5000);
    assert!(t.elapsed().as_secs() < 20, "took {:?}", t.elapsed());
}

#[test]
fn empty_run_properties_are_not_written() {
    let inner = "<w:p><w:r><w:rPr></w:rPr><w:t>Plain text</w:t></w:r><w:r><w:rPr/><w:t xml:space=\"preserve\"> and more</w:t></w:r></w:p>";
    let mut doc = round_trip(&doc_of(inner));
    doc.target_lang = Some("es-ES".into());
    doc.set_target("p1", vec![Inline::text("Texto simple y mas")])
        .unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<w:t>Texto simple y mas</w:t>"));
    assert!(!x.contains("<w:rPr></w:rPr>"), "{x}");
    assert!(!x.contains("<w:rPr/>"), "{x}");
}

#[test]
fn ruby_base_text_is_the_unit_text_and_the_guide_is_kept() {
    let ruby = "<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val=\"distributeSpace\"/><w:hps w:val=\"10\"/><w:lid w:val=\"ja-JP\"/></w:rubyPr>\
<w:rt><w:r><w:rPr><w:sz w:val=\"10\"/></w:rPr><w:t>\u{3068}\u{3046}\u{304D}\u{3087}\u{3046}</w:t></w:r></w:rt>\
<w:rubyBase><w:r><w:t>\u{6771}\u{4EAC}</w:t></w:r></w:rubyBase></w:ruby></w:r>";
    let inner = format!("<w:p><w:r><w:t xml:space=\"preserve\">Visit </w:t></w:r>{ruby}<w:r><w:t xml:space=\"preserve\"> soon.</w:t></w:r></w:p>");
    // unchanged: byte identical (round_trip checks it)
    let mut doc = round_trip(&doc_of(&inner));
    let s = seg_of(&doc, "p1");
    assert_eq!(plain_text(&s.source), "Visit \u{6771}\u{4EAC} soon.");
    assert_eq!(tag_displays(s), vec!["<ruby>", "</ruby>"]);
    assert_eq!(doc.units[0].codes["1"].label.as_deref(), Some("ruby"));
    doc.set_target(
        "p1",
        vec![
            Inline::text("Visita "),
            Inline::tag("1", TagKind::Open, "<ruby>"),
            Inline::text("Tokio"),
            Inline::tag("1", TagKind::Close, "</ruby>"),
            Inline::text(" pronto."),
        ],
    )
    .unwrap();
    let out = Docx.merge(&doc).unwrap();
    let x = xml_of(&out, "word/document.xml");
    assert!(
        x.contains("<w:rt><w:r><w:rPr><w:sz w:val=\"10\"/></w:rPr><w:t>\u{3068}\u{3046}\u{304D}\u{3087}\u{3046}</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>Tokio</w:t></w:r></w:rubyBase></w:ruby></w:r>"),
        "{x}"
    );
    assert_eq!(src(&extract(&out), "p1"), "Visita Tokio pronto.");
    // a paragraph that is only ruby is a unit too
    let only = round_trip(&doc_of(&format!("<w:p>{ruby}</w:p>")));
    assert_eq!(src(&only, "p1"), "\u{6771}\u{4EAC}");
}

#[test]
fn drawingml_text_in_a_canvas_is_a_unit() {
    // a lockedCanvas / group shape keeps its text in a:p, not in w:txbxContent
    let shape = |t1: &str, t2: &str| {
        format!(
            "<a:sp><a:txSp><a:txBody><a:bodyPr/><a:p><a:pPr algn=\"ctr\"/><a:r><a:rPr lang=\"ru-RU\" sz=\"2000\"/><a:t>{t1}</a:t></a:r>\
<a:r><a:rPr lang=\"ru-RU\"/><a:t>{t2}</a:t></a:r><a:endParaRPr lang=\"ru-RU\"/></a:p></a:txBody></a:txSp></a:sp>"
        )
    };
    let inner = format!(
        "<w:p><w:r><w:t>Before</w:t></w:r><w:r><w:drawing><wp:anchor><a:graphic><a:graphicData uri=\"urn:x\"><a:grpSp>{}</a:grpSp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>",
        shape("Young ", "ecologists &amp; co")
    );
    let mut doc = round_trip(&doc_of(&inner));
    assert_eq!(doc.units.len(), 2, "{:?}", doc.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>());
    assert_eq!(src(&doc, "p1"), "Before");
    assert_eq!(src(&doc, "d1"), "Young ecologists & co");
    assert_eq!(doc.units[1].context.kind.as_deref(), Some("textbox"));
    // the larger first run is a formatting tag; the most common formatting is the base
    let s = seg_of(&doc, "d1");
    assert_eq!(tag_displays(s), vec!["<fmt>", "</fmt>"]);
    assert_eq!(doc.units[1].codes["1"].label.as_deref(), Some("size"));
    doc.target_lang = Some("de-DE".into());
    doc.set_target(
        "d1",
        vec![
            Inline::text("Junge <Oekologen> "),
            Inline::tag("1", TagKind::Open, "<fmt>"),
            Inline::text("und Co"),
            Inline::tag("1", TagKind::Close, "</fmt>"),
        ],
    )
    .unwrap();
    let out = Docx.merge(&doc).unwrap();
    let x = xml_of(&out, "word/document.xml");
    assert!(
        x.contains("<a:pPr algn=\"ctr\"/><a:r><a:rPr lang=\"de-DE\"/><a:t>Junge &lt;Oekologen&gt; </a:t></a:r><a:r><a:rPr lang=\"de-DE\" sz=\"2000\"/><a:t>und Co</a:t></a:r><a:endParaRPr lang=\"ru-RU\"/></a:p>"),
        "{x}"
    );
    let again = extract(&out);
    assert_eq!(src(&again, "d1"), "Junge <Oekologen> und Co");
    assert_eq!(tag_displays(seg_of(&again, "d1")), vec!["<fmt>", "</fmt>"]);
    // spelling and language marks do not split the text into tags; bold is <b>
    let marks = "<w:p><w:r><w:drawing><a:p><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>Plain </a:t></a:r>\
<a:r><a:rPr lang=\"en-GB\" err=\"1\"/><a:t>text </a:t></a:r><a:r><a:rPr lang=\"en-US\" b=\"1\"/><a:t>here</a:t></a:r></a:p></w:drawing></w:r></w:p>";
    let m = round_trip(&doc_of(marks));
    assert_eq!(tag_displays(seg_of(&m, "d1")), vec!["<b>", "</b>"]);
    assert_eq!(m.find("d1").unwrap().0.codes["1"].label.as_deref(), Some("bold"));
    // a line break in the paragraph: left alone
    let br = "<w:p><w:r><w:drawing><a:p><a:r><a:t>One</a:t></a:r><a:br/><a:r><a:t>Two</a:t></a:r></a:p></w:drawing></w:r></w:p>";
    let none = round_trip(&doc_of(br));
    assert!(none.find("d1").is_none());
}

#[test]
fn drawingml_canvas_with_a_vml_fallback_is_translated_in_both_copies() {
    // Word writes a canvas as mc:Choice (DrawingML a:p) with a VML mc:Fallback (w:p in a v:textbox)
    let choice = "<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Press </a:t></a:r><a:r><a:rPr lang=\"en-US\" b=\"1\"/><a:t>Save</a:t></a:r>\
<a:r><a:rPr lang=\"en-US\"/><a:t> to keep it</a:t></a:r></a:p>";
    let fb_runs = "<w:r><w:t xml:space=\"preserve\">Press </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>Save</w:t></w:r>\
<w:r><w:t xml:space=\"preserve\"> to keep it</w:t></w:r>";
    let alt = |fb: &str| {
        format!(
            "<w:p><w:r><w:t>Before</w:t></w:r><w:r><mc:AlternateContent><mc:Choice Requires=\"wpc\"><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"urn:x\">\
<a:sp><a:txSp><a:txBody><a:bodyPr/>{choice}</a:txBody></a:txSp></a:sp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice>\
<mc:Fallback><w:pict><v:shape><v:textbox><w:txbxContent><w:p>{fb}</w:p></w:txbxContent></v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>"
        )
    };
    let t = vec![
        Inline::tag("1", TagKind::Open, "<b>"),
        Inline::text("Guardar"),
        Inline::tag("1", TagKind::Close, "</b>"),
        Inline::text(" para conservarlo"),
    ];
    // runs line up one to one: the fallback keeps its own w:rPr per tag
    let mut doc = round_trip(&doc_of(&alt(fb_runs)));
    let ids: Vec<String> = doc.units.iter().map(|u| u.id.clone()).collect();
    assert_eq!(ids, vec!["p1", "d1"]);
    assert_eq!(src(&doc, "d1"), "Press Save to keep it");
    doc.set_target("d1", t.clone()).unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("<a:r><a:rPr lang=\"en-US\" b=\"1\"/><a:t>Guardar</a:t></a:r><a:r><a:rPr lang=\"en-US\"/><a:t> para conservarlo</a:t></a:r>"), "{x}");
    assert!(
        x.contains("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Guardar</w:t></w:r><w:r><w:t xml:space=\"preserve\"> para conservarlo</w:t></w:r></w:p>"),
        "{x}"
    );
    assert!(!x.contains("Press") && !x.contains("keep it"), "{x}");
    assert_eq!(src(&extract(&Docx.merge(&doc).unwrap()), "d1"), "Guardar para conservarlo");

    // same text in one fallback run: mirrored with that run's formatting for everything
    let one = "<w:r><w:rPr><w:i/></w:rPr><w:t>Press Save to keep it</w:t></w:r>";
    let mut doc = round_trip(&doc_of(&alt(one)));
    doc.set_target("d1", t.clone()).unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(
        x.contains("<w:p><w:r><w:rPr><w:i/></w:rPr><w:t>Guardar</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t xml:space=\"preserve\"> para conservarlo</w:t></w:r></w:p>"),
        "{x}"
    );

    // a fallback with other text is left as it was
    let other = "<w:r><w:t>Something else</w:t></w:r>";
    let mut doc = round_trip(&doc_of(&alt(other)));
    doc.set_target("d1", t).unwrap();
    let x = xml_of(&Docx.merge(&doc).unwrap(), "word/document.xml");
    assert!(x.contains("Guardar") && x.contains("<w:t>Something else</w:t>"), "{x}");
}