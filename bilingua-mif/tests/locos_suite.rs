//! Ports of every case in the LocOS suite `tests/verify_mif_filter.py` (the fixtures are the suite's own).
//! LocOS has 8 test functions; the mapping is in the doc comment of each test below.
mod common;
use bilingua_mif::{fm_language, Mif};
use bilingua_model::*;
use common::*;

fn expected() -> Vec<(&'static str, &'static str)> {
    vec![
        ("CellBody", "Cell text"),
        ("Footnote", "See the manual."),
        ("Heading1", "Getting started"),
        (
            "Body",
            "Welcome to \u{27e6}x1\u{27e7}. Press \u{27e6}g2\u{27e7}Save\u{27e6}/g2\u{27e7} to keep your work, see \u{27e6}x3\u{27e7}.",
        ),
        (
            "Body",
            "Use `quotes', a > sign,\ta back\\slash\u{2014}and \u{e9}t\u{e9} caf\u{e9} done\u{27e6}x1\u{27e7}\nSecond line.",
        ),
    ]
}

fn disp(u: &Unit, id: &str, kind: TagKind) -> String {
    u.source()
        .iter()
        .find_map(|i| match i {
            Inline::Tag { id: i, kind: k, display, .. } if i == id && *k == kind => Some(display.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no tag {id} {kind:?}"))
}

/// LocOS `test_registered`: the filter is known as `mif` with the `.mif` extension.
#[test]
fn registered() {
    assert_eq!(Mif.id(), "mif");
    assert_eq!(Mif.extensions(), &["mif"]);
    assert!(!Mif.bilingual());
    assert!(Mif.sniff(SUITE));
    assert!(!Mif.sniff(b"<html></html>"));
    assert!(!Mif.sniff(b"<Book 2019>\n"));
}

/// LocOS `test_units_and_identity`.
#[test]
fn units_and_identity() {
    let got = units(SUITE);
    let want: Vec<(String, String)> = expected().into_iter().map(|(a, b)| (a.into(), b.into())).collect();
    assert_eq!(got, want);
    assert_eq!(bilingua_mif::mif_version(SUITE).unwrap(), "2019");
    let doc = extract(SUITE).unwrap();
    let u = &doc.units[3];
    let x1 = disp(u, "1", TagKind::Standalone);
    assert!(x1.starts_with("<Variable") && x1.contains("Product"), "{x1}");
    let x3 = disp(u, "3", TagKind::Standalone);
    assert!(x3.starts_with("<XRef") && x3.ends_with("<XRefEnd >"), "{x3}");
    assert!(x3.contains("Getting started on page 1") && !x3.contains("<FNote 3>"), "{x3}");
    assert!(disp(u, "2", TagKind::Open).contains("Emphasis"));
    assert!(disp(u, "2", TagKind::Close).contains("<FTag `'>"));
    // identity: untouched, and translations equal to the source
    assert!(Mif.merge(&doc).unwrap() == SUITE);
    assert_identity(SUITE);
    // CRLF + BOM identity
    let mut crlf = vec![0xEF, 0xBB, 0xBF];
    crlf.extend(String::from_utf8(SUITE.to_vec()).unwrap().replace('\n', "\r\n").into_bytes());
    let want2: Vec<(String, String)> = expected().into_iter().map(|(a, b)| (a.into(), b.into())).collect();
    assert_eq!(units(&crlf), want2);
    assert_identity(&crlf);
    // MIF 8.00 header is accepted too
    let v8 = String::from_utf8(SUITE.to_vec()).unwrap().replace("<MIFFile 2019>", "<MIFFile 8.00>");
    assert_eq!(units(v8.as_bytes()).len(), expected().len());
}

/// LocOS `test_translated_round_trip`.
#[test]
fn translated_round_trip() {
    let tr = [
        (2usize, "Erste Schritte"),
        (
            3,
            "Willkommen bei \u{27e6}x1\u{27e7}. Dr\u{fc}cken Sie \u{27e6}g2\u{27e7}Speichern\u{27e6}/g2\u{27e7}, um Ihre Arbeit zu behalten, siehe \u{27e6}x3\u{27e7}.",
        ),
        (
            4,
            "Nutzen Sie `Zitate', ein > Zeichen,\tein Back\\slash\u{2014}und Caf\u{e9}\u{27e6}x1\u{27e7}\nZweite Zeile.",
        ),
    ];
    let out = build(SUITE, &tr);
    let text = String::from_utf8(out.clone()).unwrap();
    let got = units(&out);
    assert_eq!(got[2].1, tr[0].1);
    assert_eq!(got[3].1, tr[1].1);
    assert_eq!(got[4].1, tr[2].1);
    assert_eq!(got[0].1, "Cell text");
    assert_eq!(got[1].1, "See the manual.");
    assert!(text.contains("<String `Nutzen Sie \\QZitate\\q, ein \\> Zeichen,\\tein Back\\\\slash"), "{text}");
    assert!(text.contains("<Char HardReturn>") && text.contains("<MText `quotes'>"));
    assert!(text.contains("<FNote 3>") && text.contains("<XRefSrcText `1001: Heading1: Getting started'>"));
    assert!(text.contains("<PgfNumString `1.'>") && text.contains("<TextRectID 7>"));
    assert!(!text.contains("<TextRectID 8>"), "line layout of a rebuilt paragraph is reflowed");
    // untouched regions identical: everything before the TextFlow body and after it
    let src = String::from_utf8(SUITE.to_vec()).unwrap();
    let head = &src[..src.find(" <Para\n  <Unique 1001>").unwrap()];
    assert!(text.starts_with(head));
    assert!(text.ends_with(&src[src.find(" <Para\n  <PgfTag `Body'>\n  <ParaLine\n   <ATbl 1>").unwrap()..]));
    // dropped tokens are appended (anchors survive), unknown and duplicate tokens dropped
    let out2 = build_str(
        SUITE,
        &[(3, "Kurz \u{27e6}x9\u{27e7} \u{27e6}x1\u{27e7} \u{27e6}g2\u{27e7}k\u{27e6}/g2\u{27e7} \u{27e6}x1\u{27e7}")],
    );
    let p = units(out2.as_bytes())[3].1.clone();
    assert_eq!(p, "Kurz  \u{27e6}x1\u{27e7} \u{27e6}g2\u{27e7}k\u{27e6}/g2\u{27e7} ");
    assert_eq!(out2.matches("<XRefEnd >").count(), 1);
    assert_eq!(out2.matches("<FTag `Emphasis'>").count(), 1);
    assert_eq!(out2.matches("<FNote 3>").count(), 1);
    assert_eq!(out2.matches("<VariableName `Product'>").count(), 2);
    // control characters never reach a string
    let out3 = build_str(SUITE, &[(2, "A\u{0}B\u{7}C")]);
    assert!(out3.contains("<String `ABC'>"));
}

fn one_para(body: &str) -> Vec<u8> {
    format!(
        "<MIFFile 2019>\n<TextFlow\n <Para\n  <PgfTag `Body'>\n  <ParaLine\n{body}  > # end of ParaLine\n > # end of Para\n> # end of TextFlow\n"
    )
    .into_bytes()
}

fn font(props: &[&str]) -> String {
    let mut s = String::from("   <Font\n");
    for p in props {
        s.push_str(&format!("    {p}\n"));
    }
    s.push_str("   > # end of Font\n");
    s
}

fn s(t: &str) -> String {
    format!("   <String `{t}'>\n")
}

fn bold() -> String {
    font(&["<FTag `'>", "<FWeight `Bold'>", "<FLocked No>"])
}
fn regular() -> String {
    font(&["<FTag `'>", "<FWeight `Regular'>", "<FLocked No>"])
}
fn default_font() -> String {
    font(&["<FTag `'>", "<FLocked No>"])
}
fn emph() -> String {
    font(&["<FTag `Emphasis'>", "<FLocked No>"])
}
fn bold_emph() -> String {
    font(&["<FTag `Emphasis'>", "<FWeight `Bold'>", "<FLocked No>"])
}
fn size() -> String {
    font(&["<FSize  14.0 pt>"])
}
fn size2() -> String {
    font(&["<FSize  16.0 pt>"])
}

fn first(data: &[u8]) -> String {
    texts(data).into_iter().next().unwrap_or_default()
}

const O: &str = "\u{27e6}";
const C: &str = "\u{27e7}";
fn t(s: &str) -> String {
    // short-hand: [g1] -> token, [/g1], [x1]
    s.replace('[', O).replace(']', C)
}
fn idx(hay: &str, needle: &str) -> usize {
    hay.find(needle).unwrap_or_else(|| panic!("{needle:?} not in {hay:?}"))
}

/// LocOS `test_font_pairs_680`.
#[test]
fn font_pairs_680() {
    // the soak repro shape: bold on the last word, restore after the last text
    let src = one_para(&(s("Account ") + &bold() + &s("settings") + &regular()));
    assert_eq!(first(&src), t("Account [g1]settings[/g1]"));
    let doc = extract(&src).unwrap();
    assert!(disp(&doc.units[0], "1", TagKind::Open).contains("Bold"));
    assert!(disp(&doc.units[0], "1", TagKind::Close).contains("Regular"));
    assert!(build(&src, &[(0, &first(&src))]) == src);
    let out = build_str(&src, &[(0, &t("[g1]Param\u{e8}tres[/g1] du compte"))]);
    assert!(idx(&out, "Bold") < idx(&out, "Param\u{e8}tres") && idx(&out, "Param\u{e8}tres") < idx(&out, "Regular"));
    assert!(idx(&out, "Regular") < idx(&out, " du compte"));
    assert_eq!(first(out.as_bytes()), t("[g1]Param\u{e8}tres[/g1] du compte"));
    // character format restored by an empty FTag; opener before the first text pulled in
    let src = one_para(&(emph() + &s("Save") + &default_font() + &s(" your work")));
    assert_eq!(first(&src), t("[g1]Save[/g1] your work"));
    let out = build_str(&src, &[(0, &t("Ihre Arbeit [g1]speichern[/g1]"))]);
    assert!(idx(&out, "Emphasis") < idx(&out, "speichern") && idx(&out, "speichern") < out.rfind("<FTag `'>").unwrap());
    // a pair around the whole text stays outside the unit
    let src = one_para(&(bold() + &s("Whole") + &regular()));
    assert_eq!(first(&src), "Whole");
    // nested: emphasis inside bold, restored to bold, then to the paragraph font
    let src = one_para(&(s("A ") + &bold() + &s("b ") + &bold_emph() + &s("c") + &bold() + &s(" d") + &default_font() + &s(" e")));
    assert_eq!(first(&src), t("A [g1]b [g2]c[/g2] d[/g1] e"));
    let out = build_str(&src, &[(0, &t("E [g1]D [g2]C[/g2][/g1] A"))]);
    assert_eq!(first(out.as_bytes()), t("E [g1]D [g2]C[/g2][/g1] A"));
    // an emptied pair is dropped; a point token inside a pair is kept
    let src = one_para(
        &(s("x ") + &bold() + &s("y ") + "   <Marker\n    <MType 2>\n   >\n" + &s("z") + &regular() + &s(" w")),
    );
    assert_eq!(first(&src), t("x [g1]y [x2]z[/g1] w"));
    let out = build_str(&src, &[(0, &t("[g1][/g1]X [x2]W"))]);
    assert!(!out.contains("Bold") && out.contains("<MType 2>"), "{out}");
    // a change never restored (or not restored exactly) stays a point token
    let src = one_para(&(s("a ") + &size() + &s("b ") + &size2() + &s("c")));
    assert_eq!(first(&src), t("a [x1]b [x2]c"));
    // ... unless the paragraph format says 16 pt is the paragraph's own size
    let cat = "<PgfCatalog\n <Pgf\n  <PgfTag `Body'>\n  <PgfFont\n   <FSize  16.0 pt>\n  >\n >\n>\n";
    let src2 = String::from_utf8(src.clone()).unwrap().replace("<TextFlow", &format!("{cat}<TextFlow"));
    assert_eq!(first(src2.as_bytes()), t("a [g1]b [/g1]c"));
    // a pair the translation breaks loses its font change and keeps its words, never an error; a pair whose
    // enclosing pair is dropped is dropped too
    let src = one_para(&(s("A ") + &bold() + &s("b ") + &bold_emph() + &s("c") + &bold() + &s(" d") + &default_font() + &s(" e")));
    for (tr, want) in [
        ("A [g1]b c d[/g1] e", "A [g1]b c d[/g1] e"),
        ("A [g1]b [g2]c[/g2] d[/g1] [g1]e", "A [g1]b [g2]c[/g2] d[/g1] e"),
        ("A [g1]b [g2]c[/g1] d[/g2] e", "A b c d e"),
        ("A [g1]b[/g1] [g2]c[/g2] d e", "A [g1]b[/g1] c d e"),
        ("A [x1]b [g2]c[/g2] d[/x1] e", "A b c d e"),
        ("A [/g1]b [g2]c[/g2] d[g1] e", "A b c d e"),
    ] {
        let out = build(&src, &[(0, &t(tr))]);
        assert_eq!(first(&out), t(want), "{tr}");
    }
}

const LANG_MIF: &str = r#"<MIFFile 2019>
<PgfCatalog
 <Pgf
  <PgfTag `Body'>
  <PgfLanguage USEnglish>
  <PgfFont
   <FLanguage USEnglish>
  > # end of PgfFont
 > # end of Pgf
> # end of PgfCatalog
<TextFlow
 <Para
  <PgfTag `Body'>
  <Odd =x> <Next 1>
  <ParaLine
   <String `Hello <PgfLanguage USEnglish\> world'>
   <Font
    <FLanguage UKEnglish>
   > # end of Font
   <String `again'>
  > # end of ParaLine
 > # end of Para
> # end of TextFlow
"#;

/// LocOS `test_language_stamp`. Differs by the lead's decision of 11-04: a merge where nothing is translated
/// returns the original bytes even with a target language (LocOS `build_localized` stamps it anyway).
#[test]
fn language_stamp() {
    let src = LANG_MIF.as_bytes();
    assert_eq!(texts(src), vec![t("Hello <PgfLanguage USEnglish> world[x1]again")]);
    assert_identity(src);
    assert!(build_for(src, &[], None) == src);
    assert!(build_for(src, &[], Some("de-DE")) == src, "nothing translated: original bytes");
    let out = String::from_utf8(build_for(src, &[(0, &t("Hallo Welt[x1]wieder"))], Some("de-DE"))).unwrap();
    assert!(!out.replace("USEnglish\\>", "").contains("USEnglish>") && !out.contains("UKEnglish"), "{out}");
    assert_eq!(out.matches("<PgfLanguage German>").count(), 1);
    assert_eq!(out.matches("<FLanguage German>").count(), 2);
    assert!(out.contains("<Odd =x> <Next 1>"));
    assert_eq!(texts(out.as_bytes()), vec![t("Hallo Welt[x1]wieder")]);
    // a translated merge without a target language leaves the language words alone
    let plain = String::from_utf8(build(src, &[(0, &t("Hallo Welt[x1]wieder"))])).unwrap();
    assert!(plain.contains("<PgfLanguage USEnglish>") && plain.contains("UKEnglish"));
    // BOM + CRLF kept; stamping works with an echo translation
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend(LANG_MIF.replace('\n', "\r\n").into_bytes());
    let echo = texts(&bom)[0].clone();
    let out2 = build_for(&bom, &[(0, &echo)], Some("pt-BR"));
    assert!(out2.starts_with(b"\xef\xbb\xbf<MIFFile"));
    assert!(String::from_utf8(out2).unwrap().contains("<PgfLanguage Brazilian>\r\n"));
    for (loc, want) in [
        ("en-GB", "UKEnglish"),
        ("en-US", "USEnglish"),
        ("de-CH", "SwissGerman"),
        ("fr_CA", "CanadianFrench"),
        ("zh-Hant-TW", "TraditionalChinese"),
        ("zh-TW", "TraditionalChinese"),
        ("zh-Hans", "SimplifiedChinese"),
        ("nb-NO", "Norwegian"),
        ("he", "Hebrew"),
        ("th-TH", "NoLanguage"),
    ] {
        assert_eq!(fm_language(loc), Some(want), "{loc}");
    }
    assert_eq!(fm_language(""), None);
}

fn err_text(e: &Error) -> String {
    e.to_string()
}

/// LocOS `test_hostile_inputs`.
#[test]
fn hostile_inputs() {
    let suite = String::from_utf8(SUITE.to_vec()).unwrap();
    let e = refused(suite.replace("<MIFFile 2019>", "<MIFFile 7.00>").as_bytes());
    assert!(matches!(e, Error::Encoding(_)) && err_text(&e).contains("MIF 7"), "{e}");
    let e = refused(suite.replace("<MIFFile 2019>", "<MIFFile 5.50>").as_bytes());
    assert!(matches!(e, Error::Encoding(_)) && err_text(&e).contains("FrameMaker 8"), "{e}");
    let e = refused(b"<Book 2019>\n<BookComponent>\n");
    assert!(err_text(&e).contains("chapter"), "{e}");
    let e = refused(b"hello world");
    assert!(err_text(&e).contains("MIFFile"), "{e}");
    let (latin, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.encode("<MIFFile 2019>\n<Para <ParaLine <String `caf\u{e9}'>>>");
    let e = refused(&latin);
    assert!(matches!(e, Error::Encoding(_)) && err_text(&e).contains("UTF-8"), "{e}");
    let e = refused(b"<MIFFile 2019>\n<Para <ParaLine <String `open>>>\n");
    assert!(err_text(&e).contains("could not be read"), "{e}");
    let e = refused(b"<MIFFile 2019>\n<Para <ParaLine >\n");
    assert!(err_text(&e).contains("never closed"), "{e}");
    let e = refused(b"<MIFFile 2019>\n>>\n");
    assert!(err_text(&e).contains("unbalanced"), "{e}");
    let mut deep = b"<MIFFile 2019>\n".to_vec();
    deep.extend(b"<A ".repeat(300));
    deep.extend(b">".repeat(300));
    let e = refused(&deep);
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlDepth) && err_text(&e).contains("nested"), "{e}");
    // caps from the limits in ExtractOptions
    let mut o = opts();
    o.limits = Some(Limits::new().with_max_units(2));
    let paras: String = (0..3).map(|i| format!("<Para <ParaLine <String `Para {i}'>>>\n")).collect();
    let e = Mif.extract(format!("<MIFFile 2019>\n{paras}").as_bytes(), &o).unwrap_err();
    assert!(err_text(&e).contains("segments"), "{e}");
    let e = Mif.extract(format!("<MIFFile 2019>\n{}", "<X 1>".repeat(401)).as_bytes(), &o).unwrap_err();
    assert!(err_text(&e).contains("statements"), "{e}");
    let max = Limits::default().max_unit_chars;
    let big = format!("<MIFFile 2019>\n<Para <ParaLine <String `{}'>>>", "x".repeat(max + 1));
    assert!(matches!(refused(big.as_bytes()), Error::Limit(_)));
    // a source paragraph that already holds marker characters is skipped, not mangled
    let doc = extract("<MIFFile 2019>\n<Para <ParaLine <String `a \u{27e6}x1\u{27e7} b'>>>\n".as_bytes()).unwrap();
    assert!(doc.units.is_empty());
    assert!(doc.warnings.iter().any(|w| w.kind == "skipped_marker_paragraphs"));
    // pathological escapes and long text stay linear
    let mut bigx = b"<MIFFile 2019>\n<Para <ParaLine <String `".to_vec();
    bigx.extend(b"\\x".repeat(40000));
    bigx.extend(b"'>>>\n");
    let t0 = std::time::Instant::now();
    assert_eq!(extract(&bigx).unwrap().units.len(), 1);
    assert!(t0.elapsed().as_secs() < 5);
}

/// LocOS `test_api_surface`, the part that is about the filter: a pseudo-translated file reads back with the same
/// units, keeps variables and images, and MIF 7 is refused with a message naming MIF 7. (The upload, CLI and
/// download legs belong to the LocOS web application.)
#[test]
fn api_surface_filter_part() {
    let doc = extract(SUITE).unwrap();
    let all: Vec<String> = doc.units.iter().map(|u| format!("{} {}", "\u{bb}", locos_text(u))).collect();
    let map: Vec<(usize, &str)> = all.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    let out = build(SUITE, &map);
    let back = units(&out);
    assert_eq!(back.len(), expected().len());
    assert!(back[2].1.contains("Getting started") && back[2].1 != "Getting started");
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("<VariableName `Product'>") && text.contains("=FrameImage"));
    let e = refused(b"<MIFFile 7.00>\n<Para>\n");
    assert!(err_text(&e).contains("MIF 7"));
}

/// LocOS `test_upload_accept_lists` is a check of the LocOS web templates and connector map; the Bilingua side of
/// it is the extension list.
#[test]
fn extensions_listed() {
    assert!(Mif.extensions().contains(&"mif"));
}
