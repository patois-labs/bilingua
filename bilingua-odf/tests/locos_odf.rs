//! The Localization OS suite `tests/verify_odf_filter.py`, case by case (12 of 12): registry, odt units, identity,
//! translated round trip, spaces, tolerant rebuild, ods, odp / odg, language stamps, the two right-to-left cases of
//! #724, and the hostile-input case. Where LocOS rebuilds with `build_localized(data, texts, locale)`, a Bilingua
//! merge translates the units and sets `Document::target_lang`; a merge with nothing translated returns the
//! original bytes (lead decision 11-04), so each language case translates at least one unit.
mod common;
use bilingua_model::*;
use bilingua_odf::*;
use common::*;

fn expected_odt() -> Vec<(usize, &'static str)> {
    vec![
        (0, "Getting started"),
        (1, "Press \u{27e6}g1\u{27e7}Save\u{27e6}/g1\u{27e7} to keep {0} files.\u{27e6}x2\u{27e7}"),
        (2, "A footnote about %1$s."),
        (3, "Two   gaps\u{27e6}x1\u{27e7}and a \u{27e6}g2\u{27e7}link\u{27e6}/g2\u{27e7}\u{27e6}x3\u{27e7}."),
        (4, "Before\u{27e6}x1\u{27e7} after \u{27e6}x2\u{27e7}insert\u{27e6}x3\u{27e7}.\u{27e6}x4\u{27e7}"),
        (5, "Caption\u{27e6}x1\u{27e7}"),
        (6, "Inside the box"),
        (7, "Cell text"),
        (10, "Unicode caf\u{e9} \u{65e5}\u{672c} \u{1F600}"),
        (11, "Header line"),
        (12, "Page \u{27e6}x1\u{27e7}"),
    ]
}

fn expected_owned() -> Vec<(usize, String)> {
    expected_odt().into_iter().map(|(i, t)| (i, t.to_string())).collect()
}

#[test]
fn registered() {
    let all = filters();
    let ids: Vec<&str> = all.iter().map(|f| f.id()).collect();
    assert_eq!(ids, ["odt", "ods", "odp", "odg"]);
    let exts: Vec<&str> = all.iter().flat_map(|f| f.extensions().iter().copied()).collect();
    assert_eq!(exts, ["odt", "ott", "ods", "ots", "odp", "otp", "odg", "otg"]);
    assert!(Odt::default().sniff(&odt()));
    assert!(!Ods::default().sniff(&odt()));
    assert!(!Odt::default().sniff(b"PK\x03\x04 other"));
    assert!(!Odt::default().sniff(b"plain text"));
}

#[test]
fn odt_units() {
    assert_eq!(units("odt", &odt()), expected_owned());
    // 5 "Caption" has letters; the deleted region, the comment, the float cell, the blank and the digits-only
    // paragraphs are not units (8 and 9 are counted but skipped)
    let d = extract("odt", &odt(), None);
    assert_eq!(d.format, "odt");
    assert_eq!(d.units[0].context.kind.as_deref(), Some("heading"));
    assert_eq!(d.units.last().unwrap().context.extra["part"], "styles.xml");
    assert_eq!(d.units[7].context.kind.as_deref(), Some("cell"));
    assert_eq!(d.units[1].context.style.as_deref(), Some("P1"));
    assert_eq!(d.units[1].context.heading.as_deref(), Some("Getting started"));
}

#[test]
fn identity() {
    let data = odt();
    let f = Odt::default();
    let d = f.extract(&data, &opts(None)).unwrap();
    assert_eq!(f.merge(&d).unwrap(), data, "no translation returns the original bytes");
    // an identity translation keeps every unit and rewrites nothing
    let mut same = d.clone();
    for (i, t) in expected_odt() {
        set(&mut same, i, t);
    }
    let out = f.merge(&same).unwrap();
    assert_eq!(units("odt", &out), expected_owned());
    assert_eq!(out, data);
    // a real translation: mimetype first and stored with no extra field, members in order, untouched bytes kept
    let out = build("odt", &data, &[(0, "Erste Schritte")], None);
    let (a, b) = (entries(&data), entries(&out));
    assert_eq!(a.iter().map(|e| &e.name).collect::<Vec<_>>(), b.iter().map(|e| &e.name).collect::<Vec<_>>());
    assert_eq!(b[0].name, "mimetype");
    assert!(b[0].meta.stored);
    for n in ["Pictures/a.png", "META-INF/manifest.xml", "meta.xml", "mimetype", "styles.xml"] {
        assert_eq!(member(&data, n), member(&out, n), "{n}");
    }
    assert!(b.iter().find(|e| e.name == "Pictures/a.png").unwrap().meta.stored);
    assert_ne!(member(&data, "content.xml"), member(&out, "content.xml"));
    // the raw bytes start with the mimetype member, readable without unzipping: no extra field, stored
    assert_eq!(&out[30..38], b"mimetype");
    assert_eq!(&out[28..30], &[0, 0], "extra field length");
    assert_eq!(&out[8..10], &[0, 0], "stored");
    assert_eq!(&out[38..77], b"application/vnd.oasis.opendocument.text");
}

#[test]
fn translated() {
    let data = odt();
    let out = build(
        "odt",
        &data,
        &[
            (0, "Erste Schritte"),
            (1, "Klicken Sie auf \u{27e6}g1\u{27e7}Speichern\u{27e6}/g1\u{27e7}, um {0} Dateien zu behalten.\u{27e6}x2\u{27e7}"),
            (2, "Eine Fu\u{df}note zu %1$s."),
            (3, "Zwei  L\u{fc}cken\u{27e6}x1\u{27e7}und ein \u{27e6}g2\u{27e7}Link\u{27e6}/g2\u{27e7}\u{27e6}x3\u{27e7}.\nNeu\tTab"),
            (6, "Im Kasten"),
            (7, "Zelltext"),
            (11, "Kopfzeile"),
        ],
        None,
    );
    let got: std::collections::BTreeMap<usize, String> = units("odt", &out).into_iter().collect();
    assert_eq!(got[&0], "Erste Schritte");
    assert_eq!(got[&1], "Klicken Sie auf \u{27e6}g1\u{27e7}Speichern\u{27e6}/g1\u{27e7}, um {0} Dateien zu behalten.\u{27e6}x2\u{27e7}");
    assert_eq!(got[&2], "Eine Fu\u{df}note zu %1$s.");
    assert_eq!(got[&3], "Zwei  L\u{fc}cken\u{27e6}x1\u{27e7}und ein \u{27e6}g2\u{27e7}Link\u{27e6}/g2\u{27e7}\u{27e6}x3\u{27e7}.\u{27e6}x4\u{27e7}Neu\u{27e6}x5\u{27e7}Tab");
    assert_eq!((got[&6].as_str(), got[&7].as_str(), got[&11].as_str()), ("Im Kasten", "Zelltext", "Kopfzeile"));
    assert_eq!(got[&5], "Caption\u{27e6}x1\u{27e7}");
    assert_eq!(got[&12], "Page \u{27e6}x1\u{27e7}");
    let content = member_str(&out, "content.xml");
    assert!(content.contains(r#"<text:span text:style-name="T1">Speichern</text:span>"#));
    assert!(content.contains(r#"text:note-class="footnote"><text:note-citation>1</text:note-citation>"#));
    assert!(content.contains("Zwei <text:s/>L\u{fc}cken<text:tab/>und ein "), "{content}");
    assert!(content.contains(r#"<text:a xlink:href="https://x.example/">Link</text:a><text:bookmark text:name="b1"/>."#));
    assert!(content.contains(".<text:line-break/>Neu<text:tab/>Tab</text:p>"));
    assert!(content.contains("<text:p>Deleted words</text:p>"), "tracked deletion untouched");
    assert!(content.contains("<text:p>Comment text</text:p>"));
    assert!(member_str(&out, "styles.xml").contains("Kopfzeile"));
}

#[test]
fn spaces() {
    let c = odt_content().replace("<text:p>   </text:p>", "<text:p>  lead\n  wrapped   text <text:s text:c=\"999999999\"/>x</text:p>");
    let data = Pkg::new(c).styles(odt_styles()).build();
    let got: std::collections::BTreeMap<usize, String> = units("odt", &data).into_iter().collect();
    let text = &got[&8];
    assert!(text.starts_with("lead wrapped text ") && text.ends_with('x'), "{}", &text[..40]);
    assert_eq!(text.matches(' ').count(), 3 + 1000, "text:c is capped");
    let out = build("odt", &data, &[(8, "  Anfang  zwei")], None);
    let back: std::collections::BTreeMap<usize, String> = units("odt", &out).into_iter().collect();
    assert_eq!(back[&8], "  Anfang  zwei");
    let content = member_str(&out, "content.xml");
    assert!(content.contains(r#"<text:p><text:s text:c="2"/>Anfang <text:s/>zwei</text:p>"#), "{content}");
}

#[test]
fn tolerant() {
    let data = odt();
    let f = Odt::default();
    let mut d = f.extract(&data, &opts(None)).unwrap();
    // unknown, repeated and kind-mismatched codes, written straight into the target (set_target would refuse them)
    let tag = |id: &str, kind: TagKind| Inline::tag(id, kind, "?");
    let u = d.units.iter_mut().find(|u| p_index(u) == 1).unwrap();
    u.segments[0].target = vec![
        Inline::text("Nur Text "),
        tag("9", TagKind::Open),
        Inline::text("fremd"),
        tag("9", TagKind::Close),
        Inline::text(" "),
        tag("1", TagKind::Standalone),
        Inline::text(" "),
        tag("7", TagKind::Standalone),
        Inline::text(" "),
        tag("4", TagKind::Close),
        Inline::text(" Ende"),
    ];
    let out = f.merge(&d).unwrap();
    let content = member_str(&out, "content.xml");
    // the footnote (x2) was dropped by the translation but is kept at the end
    let p = content.split(r#"text:style-name="P1">"#).nth(1).unwrap();
    assert!(p.starts_with("Nur Text fremd  "), "{p}");
    assert!(!content.contains('\u{27e6}'));
    assert!(content.contains(r#"text:id="ftn1""#) && content.contains("A footnote about"));
    assert!(!content.contains("Speichern"));
    assert!(!p.split("</text:p>").next().unwrap().contains("Save"));
    let got: std::collections::BTreeMap<usize, String> = units("odt", &out).into_iter().collect();
    assert_eq!(got[&2], "A footnote about %1$s.");
}

fn ods_content() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}",
        concat!(
            "<office:document-content NSDECL><office:body><office:spreadsheet>",
            r#"<table:table table:name="Sheet1"><table:table-row>"#,
            r#"<table:table-cell office:value-type="string"><text:p>Name</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="float" office:value="42"><text:p>42</text:p>"#,
            "</table:table-cell>",
            r#"<table:table-cell table:formula="of:=CONCAT(&quot;a&quot;;&quot;b&quot;)" "#,
            r#"office:value-type="string" office:string-value="ab"><text:p>ab</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="date" office:date-value="2026-10-10">"#,
            "<text:p>Oct 10, 2026</text:p></table:table-cell>",
            r#"<table:table-cell table:number-columns-repeated="3" office:value-type="string">"#,
            "<text:p>Repeated label</text:p></table:table-cell>",
            r#"<table:table-cell office:value-type="string"><text:p>Total <text:span>due</text:span></text:p>"#,
            "<office:annotation><text:p>cell comment</text:p></office:annotation></table:table-cell>",
            "</table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"
        )
        .replace("NSDECL", NSDECL)
    )
}

#[test]
fn ods() {
    let data = Pkg::new(ods_content()).mime(MIME_SHEET).build();
    let want: Vec<(usize, String)> = vec![(0, "Name".into()), (1, "Repeated label".into()), (2, "Total \u{27e6}g1\u{27e7}due\u{27e6}/g1\u{27e7}".into())];
    assert_eq!(units("ods", &data), want);
    let out = build("ods", &data, &[(0, "Nom"), (1, "\u{c9}tiquette"), (2, "Total \u{27e6}g1\u{27e7}d\u{fb}\u{27e6}/g1\u{27e7}")], None);
    let c = member_str(&out, "content.xml");
    let rep = r#"<table:table-cell table:number-columns-repeated="3" office:value-type="string">"#;
    assert!(c.contains(&format!("{rep}<text:p>\u{c9}tiquette")));
    assert!(c.contains("<text:p>42</text:p>") && c.contains("<text:p>Oct 10, 2026</text:p>"));
    assert!(c.contains(r#"office:string-value="ab"><text:p>ab</text:p>"#));
    assert!(c.contains("<text:p>cell comment</text:p>"));
    assert_eq!(member(&out, "mimetype"), b"application/vnd.oasis.opendocument.spreadsheet");
}

pub fn odp_content() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}",
        concat!(
            "<office:document-content NSDECL><office:body><office:presentation>",
            r#"<draw:page draw:name="Slide 1"><draw:frame presentation:class="title"><draw:text-box>"#,
            "<text:p>Quarterly results</text:p></draw:text-box></draw:frame>",
            r#"<draw:frame presentation:class="outline"><draw:text-box><text:list><text:list-item>"#,
            "<text:p>Revenue <text:span>up</text:span> 20%</text:p></text:list-item></text:list>",
            "</draw:text-box></draw:frame>",
            "<presentation:notes><draw:frame><draw:text-box><text:p>Speak slowly here</text:p>",
            "</draw:text-box></draw:frame></presentation:notes></draw:page>",
            "</office:presentation></office:body></office:document-content>"
        )
        .replace("NSDECL", NSDECL)
    )
}

pub fn odp_styles() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}",
        concat!(
            "<office:document-styles NSDECL><office:master-styles>",
            r#"<style:master-page style:name="Default"><draw:frame presentation:class="title">"#,
            "<draw:text-box><text:p>Click to edit the title</text:p></draw:text-box></draw:frame>",
            "</style:master-page></office:master-styles></office:document-styles>"
        )
        .replace("NSDECL", NSDECL)
    )
}

fn odg_content() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}",
        concat!(
            "<office:document-content NSDECL><office:body><office:drawing>",
            r#"<draw:page draw:name="page1"><draw:custom-shape><text:p>Start here</text:p>"#,
            "</draw:custom-shape><draw:rect><text:p>Decision</text:p></draw:rect></draw:page>",
            "</office:drawing></office:body></office:document-content>"
        )
        .replace("NSDECL", NSDECL)
    )
}

#[test]
fn odp_odg() {
    let data = Pkg::new(odp_content()).styles(odp_styles()).mime(MIME_SLIDES).build();
    let want: Vec<(usize, String)> = vec![
        (0, "Quarterly results".into()),
        (1, "Revenue \u{27e6}g1\u{27e7}up\u{27e6}/g1\u{27e7} 20%".into()),
        (2, "Speak slowly here".into()),
    ];
    assert_eq!(units("odp", &data), want);
    let out = build("odp", &data, &[(2, "Langsam sprechen")], None);
    assert!(member_str(&out, "content.xml").contains("Langsam sprechen"));
    assert_eq!(member(&out, "styles.xml"), member(&data, "styles.xml"));
    let g = Pkg::new(odg_content()).mime(MIME_DRAW).build();
    assert_eq!(units("odg", &g), vec![(0, "Start here".to_string()), (1, "Decision".to_string())]);
    let out = build("odg", &g, &[(1, "Entscheidung")], None);
    assert_eq!(units("odg", &out), vec![(0, "Start here".to_string()), (1, "Entscheidung".to_string())]);
}

fn default_style<'a>(styles: &'a str, family: &str) -> &'a str {
    element(styles, &format!(r#"<style:default-style style:family="{family}">"#), "</style:default-style>")
}

#[test]
fn localized() {
    let data = odt();
    let out = build("odt", &data, &[(0, "Erste Schritte")], Some("de_DE"));
    let ds = default_style(&member_str(&out, "styles.xml"), "paragraph").to_string();
    assert!(ds.contains(r#"fo:language="de""#) && ds.contains(r#"fo:country="DE""#), "{ds}");
    assert!(member_str(&out, "meta.xml").contains("<dc:language>de-DE</dc:language>"));
    // East Asian: the -asian slot; fo:language keeps the file's language
    let ja = build("odt", &data, &[(0, "\u{306f}\u{3058}\u{3081}\u{306b}")], Some("ja"));
    let ds = default_style(&member_str(&ja, "styles.xml"), "paragraph").to_string();
    assert!(ds.contains(r#"style:language-asian="ja""#) && ds.contains(r#"fo:language="en""#), "{ds}");
    // #730: Thai / Hindi are complex-script text: the -complex slot, not fo:language
    for (loc, lang) in [("th-TH", "th"), ("hi-IN", "hi")] {
        let o = build("odt", &data, &[(0, "x")], Some(loc));
        let ds = default_style(&member_str(&o, "styles.xml"), "paragraph").to_string();
        assert!(ds.contains(&format!(r#"style:language-complex="{lang}""#)), "{loc}: {ds}");
        assert!(ds.contains(r#"fo:language="en""#), "{loc}: {ds}");
    }
    let ar = build("odt", &data, &[(0, "\u{627}\u{644}\u{628}\u{62f}\u{621}")], Some("ar"));
    let ds = default_style(&member_str(&ar, "styles.xml"), "paragraph").to_string();
    assert!(ds.contains(r#"style:language-complex="ar""#) && ds.contains(r#"style:writing-mode="rl-tb""#), "{ds}");
    let mut want = expected_owned();
    want[0].1 = "\u{627}\u{644}\u{628}\u{62f}\u{621}".into();
    assert_eq!(units("odt", &ar), want);
    // no target language, or a placeholder: nothing is stamped
    let none = build("odt", &data, &[(0, "Erste")], None);
    assert_eq!(member(&none, "styles.xml"), member(&data, "styles.xml"));
    assert_eq!(member(&none, "meta.xml"), member(&data, "meta.xml"));
    let dash = build("odt", &data, &[(0, "Erste")], Some("-"));
    assert_eq!(member(&dash, "styles.xml"), member(&data, "styles.xml"));
    // nothing translated: the original bytes, even with a target language
    assert_eq!(build("odt", &data, &[], Some("de-DE")), data);
    // Gate: a "[None]" language style (zxx / none, code text) keeps proofing off; a style that declares a real
    // language is re-stamped.
    let styles = odt_styles().replace(
        r#"<style:style style:name="Standard" style:family="paragraph"/>"#,
        concat!(
            r#"<style:style style:name="Code" style:family="text"><style:text-properties fo:language="zxx" fo:country="none"/></style:style>"#,
            r#"<style:style style:name="Quote" style:family="text"><style:text-properties fo:language="en" fo:country="GB"/></style:style>"#
        ),
    );
    let out = build("odt", &Pkg::new(odt_content()).styles(styles).build(), &[(0, "Erste")], Some("de-DE"));
    let s = member_str(&out, "styles.xml");
    let code = element(&s, r#"<style:style style:name="Code""#, "</style:style>");
    let quote = element(&s, r#"<style:style style:name="Quote""#, "</style:style>");
    assert!(code.contains(r#"fo:language="zxx""#) && code.contains(r#"fo:country="none""#), "{code}");
    assert!(quote.contains(r#"fo:language="de""#) && quote.contains(r#"fo:country="DE""#), "{quote}");
}

pub fn odp_styles_graphic() -> String {
    odp_styles().replace(
        "<office:master-styles>",
        concat!(
            r##"<office:styles><style:default-style style:family="graphic">"##,
            r##"<style:graphic-properties svg:stroke-color="#3465a4"/>"##,
            r#"<style:paragraph-properties style:text-autospace="ideograph-alpha" style:writing-mode="lr-tb"/>"#,
            r#"<style:text-properties fo:language="en" fo:country="US"/></style:default-style>"#,
            r#"<style:default-style style:family="presentation"><style:text-properties fo:language="en" fo:country="US"/></style:default-style>"#,
            "</office:styles><office:master-styles>"
        ),
    )
}

fn pos(s: &str, needle: &str) -> usize {
    s.find(needle).unwrap_or_else(|| panic!("no {needle} in {s}"))
}

#[test]
fn rtl_frames_724() {
    let data = Pkg::new(odp_content()).styles(odp_styles_graphic()).mime(MIME_SLIDES).build();
    for loc in ["he-IL", "ar-SA", "fa"] {
        let out = build("odp", &data, &[(0, "\u{5ea}\u{5d5}\u{5e6}\u{5d0}\u{5d5}\u{5ea}")], Some(loc));
        let styles = member_str(&out, "styles.xml");
        let ds = default_style(&styles, "graphic");
        assert!(pos(ds, "<style:graphic-properties") < pos(ds, "<style:paragraph-properties"), "{loc}: {ds}");
        assert!(pos(ds, "<style:paragraph-properties") < pos(ds, "<style:text-properties"), "{loc}: {ds}");
        let pp = element(ds, "<style:paragraph-properties", "/>");
        assert!(pp.contains(r#"style:writing-mode="rl-tb""#) && pp.contains(r#"style:text-autospace="ideograph-alpha""#), "{loc}: {pp}");
        assert_eq!(units("odp", &out)[1..], units("odp", &data)[1..]);
    }
    for loc in ["de-DE", "ja-JP"] {
        let out = build("odp", &data, &[(0, "Ergebnisse")], Some(loc));
        let ds = default_style(&member_str(&out, "styles.xml"), "graphic").to_string();
        assert!(element(&ds, "<style:paragraph-properties", "/>").contains(r#"style:writing-mode="lr-tb""#), "{loc}: {ds}");
    }
    // a graphic default without paragraph-properties gets one, after graphic-properties
    let bare = odp_styles_graphic().replace(
        r#"<style:paragraph-properties style:text-autospace="ideograph-alpha" style:writing-mode="lr-tb"/>"#,
        "",
    );
    let g = Pkg::new(odg_content()).styles(bare).mime(MIME_DRAW).build();
    let out = build("odg", &g, &[(1, "\u{627}\u{644}\u{642}\u{631}\u{627}\u{631}")], Some("ar"));
    let ds = default_style(&member_str(&out, "styles.xml"), "graphic").to_string();
    assert!(pos(&ds, "<style:graphic-properties") < pos(&ds, "<style:paragraph-properties"));
    assert!(pos(&ds, "<style:paragraph-properties") < pos(&ds, "<style:text-properties"));
    assert!(element(&ds, "<style:paragraph-properties", "/>").contains(r#"style:writing-mode="rl-tb""#));
}

/// Modern LibreOffice shape (a pptx saved as ODP by LibreOffice 25): every shape's graphic / presentation style and
/// the automatic paragraph styles inside it carry an explicit lr-tb, which overrides the default style.
const MODERN_AUTO: &str = concat!(
    "<office:automatic-styles>",
    r#"<style:style style:name="gr1" style:family="graphic" style:parent-style-name="standard">"#,
    r#"<style:graphic-properties draw:fill="none"/>"#,
    r#"<style:paragraph-properties style:writing-mode="lr-tb"/></style:style>"#,
    r#"<style:style style:name="pr1" style:family="presentation">"#,
    r#"<style:graphic-properties style:writing-mode="lr-tb"/>"#,
    r#"<style:paragraph-properties style:writing-mode="lr-tb"/></style:style>"#,
    r#"<style:style style:name="P1" style:family="paragraph">"#,
    r#"<style:paragraph-properties fo:text-align="left" style:writing-mode="lr-tb"/></style:style>"#,
    r#"<style:style style:name="P2" style:family="paragraph">"#,
    r#"<style:paragraph-properties style:writing-mode="tb-rl"/></style:style>"#,
    r#"<style:style style:name="P3" style:family="paragraph">"#,
    r#"<style:paragraph-properties style:writing-mode="page"/></style:style>"#,
    r#"<style:style style:name="T1" style:family="text">"#,
    r#"<style:text-properties fo:font-weight="bold"/></style:style>"#,
    "</office:automatic-styles>"
);

/// style name to the writing mode of its paragraph-properties.
fn modes(xml: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut rest = xml;
    while let Some(a) = rest.find("<style:style ") {
        let block_end = rest[a..].find("</style:style>").map_or(rest.len(), |e| a + e);
        let block = &rest[a..block_end];
        let name = block.split(r#"style:name=""#).nth(1).and_then(|s| s.split('"').next()).unwrap_or("").to_string();
        if let Some(pp) = block.find("<style:paragraph-properties") {
            let tag = &block[pp..pp + block[pp..].find('>').unwrap()];
            if let Some(m) = tag.split(r#"style:writing-mode=""#).nth(1).and_then(|s| s.split('"').next()) {
                out.insert(name, m.to_string());
            }
        }
        rest = &rest[block_end..];
    }
    out
}

#[test]
fn rtl_explicit_ltr_styles_724() {
    let content = odp_content().replace("<office:body>", &format!("{MODERN_AUTO}<office:body>"));
    let styles = odp_styles_graphic().replace(
        "</office:styles>",
        concat!(
            r#"<style:style style:name="standard" style:family="graphic">"#,
            r#"<style:paragraph-properties style:writing-mode="lr-tb"/></style:style></office:styles>"#
        ),
    );
    let data = Pkg::new(content).styles(styles).mime(MIME_SLIDES).build();
    let want: std::collections::BTreeMap<String, String> =
        [("gr1", "rl-tb"), ("pr1", "rl-tb"), ("P1", "rl-tb"), ("P2", "tb-rl"), ("P3", "page")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    for loc in ["he-IL", "ar-SA"] {
        let out = build("odp", &data, &[(0, "\u{5ea}\u{5d5}\u{5e6}\u{5d0}\u{5d5}\u{5ea}")], Some(loc));
        let c = member_str(&out, "content.xml");
        assert_eq!(modes(&c), want, "{loc}");
        let p1 = element(&c, r#"<style:style style:name="P1""#, "</style:style>");
        assert!(p1.contains(r#"fo:text-align="left""#), "alignment is not mirrored");
        let pr1 = element(&c, r#"<style:style style:name="pr1""#, "</style:style>");
        assert!(element(pr1, "<style:graphic-properties", "/>").contains(r#"style:writing-mode="lr-tb""#), "frame-level mode untouched");
        assert_eq!(modes(&member_str(&out, "styles.xml"))["standard"], "rl-tb");
        assert_eq!(units("odp", &out)[1..], units("odp", &data)[1..]);
    }
    let out = build("odp", &data, &[(0, "Ergebnisse")], Some("de-DE"));
    assert_eq!(modes(&member_str(&out, "content.xml")), modes(&member_str(&data, "content.xml")));
    assert_eq!(modes(&member_str(&out, "styles.xml")), modes(&member_str(&data, "styles.xml")));
}

fn refused(data: &[u8], needle: &str) -> Error {
    let e = Odt::default().extract(data, &opts(None)).err().unwrap_or_else(|| panic!("not refused: {needle}"));
    assert!(e.to_string().contains(needle), "{needle}: {e}");
    e
}

#[test]
fn hostile() {
    let e = refused(b"not a zip at all", "not a zip");
    assert!(matches!(e, Error::Parse { .. }));
    refused(&Pkg::new(odt_content()).mime("application/zip").build(), "not an OpenDocument");
    let enc = manifest(MIME_TEXT, &["content.xml"]).replace("</manifest:manifest>", "<manifest:encryption-data/></manifest:manifest>");
    let e = refused(&Pkg::new(odt_content()).manifest(enc).build(), "password-protected");
    assert!(matches!(e, Error::Unsupported(_)));
    let doctype = odt_content()
        .replacen("<office:document-content", r#"<!DOCTYPE x [<!ENTITY a "aaaa"><!ENTITY b "&a;&a;&a;">]><office:document-content"#, 1)
        .replace("Getting started", "&b;");
    assert!(matches!(Odt::default().extract(&Pkg::new(doctype).build(), &opts(None)), Err(Error::Limit(_))));
    // a DOCTYPE with no entity declaration is refused too
    let plain = odt_content().replacen("<office:document-content", "<!DOCTYPE x><office:document-content", 1);
    assert!(matches!(Odt::default().extract(&Pkg::new(plain).build(), &opts(None)), Err(Error::Limit(_))));
    refused(&Pkg::new(odt_content().replace("Getting started", "&ext;")).build(), "invalid XML");
    refused(&Pkg::new("<office:document-content".into()).build(), "invalid XML");
    let only_mime = write_zip(&[(EntryMeta { name: "mimetype".into(), stored: true, dir: false, modified: None }, MIME_TEXT.as_bytes().to_vec())]).unwrap();
    refused(&only_mime, "no content.xml");
    // segment cap, then the markup budget
    let tiny = |n: usize| ExtractOptions::new().with_limits(Limits::new().with_max_units(n));
    match Odt::default().extract(&odt(), &tiny(3)) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::Units),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    match Odt::default().extract(&odt(), &tiny(2)) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::XmlElements),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    let spaces = r#"<text:s text:c="1000"/>"#.repeat(200);
    match Odt::default().extract(&Pkg::new(odt_content().replace("Getting started", &spaces)).styles(odt_styles()).build(), &opts(None)) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::UnitChars),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
    let bomb = Pkg::new(odt_content()).extra("Pictures/zero.bin", vec![0u8; 8 * 1024 * 1024]).build();
    match Odt::default().extract(&bomb, &opts(None)) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::ZipRatio),
        other => panic!("{:?}", other.map(|d| d.units.len())),
    }
}

use bilingua_zip::{write_zip, EntryMeta};

/// W-D D3 (odf-x): under the locos profile a part that is rewritten is written as LocOS's lxml writes it, so the
/// package matches byte for byte. Native mode keeps the file's own declaration and quoting.
#[test]
fn locos_profile_writes_a_rewritten_part_like_lxml() {
    let ns = r#" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/""#;
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<office:document-content office:version=\"1.3\"{ns}><office:body><office:text><text:p text:style-name=\"P&apos;1\" fo:note=\"a &quot;b&quot; &gt; c\">Hallo &amp; ciao &gt; &#xE9;</text:p><text:p></text:p><text:p text:style-name=\"T\"/></office:text></office:body></office:document-content>\n"
    );
    let data = Pkg::new(content).build();
    let f = Odt::default();
    let mut d = f.extract(&data, &opts(Some("de-DE"))).unwrap();
    set(&mut d, 0, "Salut & ciao > \u{e9}");
    // LocOS: etree.tostring(root, xml_declaration=True, encoding="UTF-8")
    let want = format!(
        "<?xml version='1.0' encoding='UTF-8'?>\n<office:document-content{ns} office:version=\"1.3\"><office:body><office:text><text:p text:style-name=\"P'1\" fo:note=\"a &quot;b&quot; &gt; c\">Salut &amp; ciao &gt; \u{e9}</text:p><text:p/><text:p text:style-name=\"T\"/></office:text></office:body></office:document-content>"
    );
    let out = f.merge_with(&d, &MergeOptions::new().with_profile("locos")).unwrap();
    assert_eq!(member_str(&out, "content.xml"), want);
    let meta = member_str(&out, "meta.xml");
    assert!(meta.starts_with("<?xml version='1.0' encoding='UTF-8'?>\n<office:document-meta"), "{meta}");
    assert!(meta.contains("<dc:language>de-DE</dc:language>") && meta.ends_with("</office:document-meta>"), "{meta}");
    // native: the file's own declaration, quoting and white space stay
    let native = member_str(&f.merge(&d).unwrap(), "content.xml");
    assert!(native.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<office:document-content"), "{native}");
    assert!(native.contains("P&apos;1") && native.contains("<text:p></text:p>") && native.ends_with(">\n"), "{native}");
    // a part with a CDATA section gets the declaration only
    let cdata = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content{ns}><office:body><office:text><text:p>Hi</text:p><![CDATA[x]]></office:text></office:body></office:document-content>");
    let data = Pkg::new(cdata).build();
    let mut d = f.extract(&data, &opts(None)).unwrap();
    set(&mut d, 0, "Salut");
    let out = member_str(&f.merge_with(&d, &MergeOptions::new().with_profile("locos")).unwrap(), "content.xml");
    assert!(out.starts_with("<?xml version='1.0' encoding='UTF-8'?>\n<office:document-content") && out.contains("<![CDATA[x]]>") && out.contains("Salut"), "{out}");
}

const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";

fn h_ns(fo: &str) -> String {
    format!(
        r#" xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="{fo}" xmlns:dc="http://purl.org/dc/elements/1.1/""#
    )
}

fn h_doc(root: &str, ns: &str, inner: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<office:document-{root}{ns}>{inner}</office:document-{root}>")
}

/// W-H H1, H2, H3: Localization OS rebuilds every paragraph it is given text for, also a paragraph whose text did not
/// change. A localized build with nothing translated differently therefore still writes content.xml again (lxml
/// declaration), writes a run of spaces after text as a space and `text:s`, and drops white space between inline
/// elements. The build for no target locale with every text unchanged returns the upload.
#[test]
fn locos_profile_rebuilds_unchanged_paragraphs_like_localos() {
    let ns = h_ns(FO);
    let body = concat!(
        "<office:body><office:text><text:p>Hello<text:s text:c=\"2\"/><text:bookmark text:name=\"b\"/>done</text:p>",
        "<text:p text:style-name=\"P48\">
		<text:span text:style-name=\"T49\">Hello</text:span> <text:span text:style-name=\"T50\">world</text:span></text:p>",
        "</office:text></office:body>"
    );
    let styles = h_doc("styles", &ns, "<office:styles/>");
    let data = Pkg::new(h_doc("content", &ns, body)).styles(styles.clone()).build();
    let f = Odt::default();
    let own = |d: &mut Document| {
        for u in d.units.clone() {
            set(d, p_index(&u), &ltext(&u));
        }
    };
    let locos = MergeOptions::new().with_profile("locos");

    // a localized build: content.xml and meta.xml are written again, styles.xml (nothing to stamp) is not
    let mut d = f.extract(&data, &opts(Some("de-DE"))).unwrap();
    own(&mut d);
    let out = f.merge_with(&d, &locos).unwrap();
    let c = member_str(&out, "content.xml");
    assert!(c.starts_with("<?xml version='1.0' encoding='UTF-8'?>
<office:document-content"), "{c}");
    assert!(c.contains("<text:p>Hello <text:s/><text:bookmark text:name=\"b\"/>done</text:p>"), "{c}");
    assert!(
        c.contains("<text:p text:style-name=\"P48\"><text:span text:style-name=\"T49\">Hello</text:span> <text:span text:style-name=\"T50\">world</text:span></text:p>"),
        "{c}"
    );
    assert_eq!(member_str(&out, "styles.xml"), styles);
    assert!(member_str(&out, "meta.xml").starts_with("<?xml version='1.0' encoding='UTF-8'?>
"));

    // no target locale, every text unchanged: the upload comes back as it was
    let mut d = f.extract(&data, &opts(None)).unwrap();
    own(&mut d);
    assert_eq!(f.merge_with(&d, &locos).unwrap(), data);
    // no target locale, one text changed: every paragraph that was given text is rebuilt
    set(&mut d, 1, "Hello world!");
    let c = member_str(&f.merge_with(&d, &locos).unwrap(), "content.xml");
    assert!(c.contains("<text:p>Hello <text:s/><text:bookmark text:name=\"b\"/>done</text:p>") && c.contains("world!"), "{c}");

    // native: an unchanged paragraph keeps its markup
    let mut d = f.extract(&data, &opts(Some("de-DE"))).unwrap();
    own(&mut d);
    let c = member_str(&f.merge(&d).unwrap(), "content.xml");
    assert!(c.starts_with("<?xml version=\"1.0\"") && c.contains("<text:s text:c=\"2\"/>") && c.contains("P48\">
		<text:span"), "{c}");
}

/// W-H H1: a new attribute in a namespace the document does not bind gets the prefix lxml gives it (`ns0`), also when
/// the document uses the usual prefix for another namespace (an old `fo` URI). Native mode keeps the usual prefix
/// where it is free, and never redefines one the document binds to something else.
#[test]
fn a_new_namespace_gets_the_prefix_lxml_gives_it() {
    let old = "http://www.w3.org/1999/XSL/Format";
    let ns = h_ns(old);
    let styles = h_doc(
        "styles",
        &ns,
        r#"<office:styles><style:default-style style:family="paragraph"><style:text-properties fo:color="#000000" fo:language="en" fo:country="US"/></style:default-style></office:styles>"#,
    );
    let content = h_doc("content", &ns, "<office:body><office:text><text:p>Hi</text:p></office:text></office:body>");
    let data = Pkg::new(content).styles(styles).build();
    let f = Odt::default();
    let mut d = f.extract(&data, &opts(Some("de-DE"))).unwrap();
    set(&mut d, 0, "Salut");
    let out = f.merge_with(&d, &MergeOptions::new().with_profile("locos")).unwrap();
    let s = member_str(&out, "styles.xml");
    let want = format!(r##"<style:text-properties xmlns:ns0="{FO}" fo:color="#000000" fo:language="en" fo:country="US" ns0:language="de" ns0:country="DE"/>"##);
    assert!(s.contains(&want), "{s}");
    // native: the usual prefix is taken by the old URI, so the new one is not called `fo` either
    let s = member_str(&f.merge(&d).unwrap(), "styles.xml");
    assert!(s.contains(&format!(r#"xmlns:ns0="{FO}""#)) && s.contains(r#"ns0:language="de""#) && s.contains(r#"fo:language="en""#), "{s}");
    // where the usual prefix is free, native keeps it and the profile says `ns0`
    let bare = h_ns(FO).replace(&format!(r#" xmlns:fo="{FO}""#), "");
    let styles = h_doc(
        "styles",
        &bare,
        r#"<office:styles><style:default-style style:family="paragraph"><style:text-properties style:font-name="x"/></style:default-style></office:styles>"#,
    );
    let content = h_doc("content", &bare, "<office:body><office:text><text:p>Hi</text:p></office:text></office:body>");
    let data = Pkg::new(content).styles(styles).build();
    let mut d = f.extract(&data, &opts(Some("de-DE"))).unwrap();
    set(&mut d, 0, "Salut");
    let s = member_str(&f.merge_with(&d, &MergeOptions::new().with_profile("locos")).unwrap(), "styles.xml");
    assert!(s.contains(&format!(r#"<style:text-properties xmlns:ns0="{FO}" style:font-name="x" ns0:language="de" ns0:country="DE"/>"#)), "{s}");
    let s = member_str(&f.merge(&d).unwrap(), "styles.xml");
    assert!(s.contains(&format!(r#"<style:text-properties style:font-name="x" xmlns:fo="{FO}" fo:language="de" fo:country="DE"/>"#)), "{s}");
}
