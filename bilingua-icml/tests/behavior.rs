//! Behaviour beyond the LocOS suite: wrappers, tracked changes, several breaks in one range, bare text, dropped
//! points, escaping, right-to-left targets, line ends, the 11 languages and the encodings (UTF-8 only).

mod common;

use bilingua_icml::Icml;
use bilingua_model::*;
use common::*;

fn open(x: &str) -> String {
    x.to_string()
}

#[test]
fn hyperlink_spanning_ranges_and_inside_a_range() {
    let body = psr(&format!(
        "<HyperlinkTextSource Self=\"h1\" Name=\"l\">{}{}</HyperlinkTextSource>{}",
        csr("<Content>Click </Content>"),
        csr_with("<Content>here</Content>", BOLD, ""),
        csr("<Content> please.</Content>")
    ));
    let data = icml(&body, "", "");
    let t = units(&data);
    assert_eq!(t.len(), 1);
    // the wrapper spans whole ranges, so it stays in place (LocOS: mode B, every range tagged)
    assert_eq!(t[0], "\u{27E6}r1\u{27E7}Click \u{27E6}/r1\u{27E7}\u{27E6}r2\u{27E7}here\u{27E6}/r2\u{27E7}\u{27E6}r3\u{27E7} please.\u{27E6}/r3\u{27E7}");
    let out = translate(&data, &[(0, &t[0].replace("Click ", "Klicken Sie ").replace("here", "hier").replace("please", "bitte"))]);
    assert!(well_formed(&out));
    assert_eq!(attr_vals(&out, "HyperlinkTextSource", "Self"), vec![Some("h1".to_string())]);
    assert!(visible_text(&out).contains("hier"));
    assert_eq!(units(&out)[0], t[0].replace("Click ", "Klicken Sie ").replace("here", "hier").replace("please", "bitte"));
}

#[test]
fn tracked_insertions_are_text_deletions_are_points() {
    let body = psr(&csr(
        "<Content>Keep </Content><Change Self=\"c1\" ChangeType=\"InsertedText\"><CharacterStyleRange><Content>added</Content></CharacterStyleRange></Change><Change Self=\"c2\" ChangeType=\"DeletedText\"><CharacterStyleRange><Content>gone</Content></CharacterStyleRange></Change><Content> end</Content>",
    ));
    let data = icml(&body, "", "");
    let t = units(&data);
    assert_eq!(t.len(), 1);
    assert!(t[0].contains("added") && !t[0].contains("gone"), "{t:?}");
    let out = translate(&data, &[(0, &t[0].replace("Keep", "Behalte").replace("added", "neu"))]);
    let s = String::from_utf8(out.clone()).unwrap();
    assert!(s.contains("gone") && s.contains("c2") && s.contains("neu"), "{s}");
    assert!(well_formed(&out));
}

#[test]
fn several_breaks_in_one_range_translate_one_paragraph() {
    let body = psr(&csr("<Content>One.</Content><Br/><Content>Two.</Content><Br/><Content>Three.</Content><Br/>"));
    let data = icml(&body, "", "");
    assert_eq!(units(&data), vec!["One.", "Two.", "Three."]);
    let out = translate(&data, &[(1, "Zwei, viel laenger.")]);
    assert_eq!(units(&out), vec!["One.", "Zwei, viel laenger.", "Three."]);
    assert_eq!(count(&out, "Br"), 3);
    assert!(well_formed(&out));
}

#[test]
fn bare_content_directly_in_the_story() {
    let data = icml("\t\t<Content>Bare text</Content><Br/>", "", "");
    assert_eq!(units(&data), vec!["Bare text"]);
    let out = translate(&data, &[(0, "Nackter Text")]);
    assert_eq!(units(&out), vec!["Nackter Text"]);
    assert!(well_formed(&out));
    assert_eq!(count(&out, "CharacterStyleRange"), 1);
}

#[test]
fn dropped_points_come_back_and_text_is_escaped() {
    let body = psr(&csr(
        "<Content>Page </Content><PageReference Self=\"p1\" PageReferenceType=\"CurrentPage\"/><Content> text<?ACE 7?> end</Content>",
    ));
    let data = icml(&body, "", "");
    let t = units(&data);
    assert_eq!(t[0], "Page \u{27E6}x1\u{27E7} text\u{27E6}x2\u{27E7} end");
    // both points dropped, special characters in the text, a newline and an illegal control character
    let out = translate(&data, &[(0, "Seite & <mehr> \"A\"\nB\u{1}C")]);
    assert!(well_formed(&out));
    assert_eq!(count(&out, "PageReference"), 1, "a dropped point is appended, never lost");
    let s = String::from_utf8(out.clone()).unwrap();
    assert!(s.contains("<?ACE 7?>"));
    assert!(visible_text(&out).starts_with("Seite & <mehr> \"A\" BC"), "{}", visible_text(&out));
    assert!(s.contains("&amp;") && s.contains("&lt;mehr&gt;"));
    // a duplicated or invented point is ignored
    let out2 = translate(&data, &[(0, "\u{27E6}x1\u{27E7}\u{27E6}x1\u{27E7}\u{27E6}x9\u{27E7}Hallo\u{27E6}/g5\u{27E7}")]);
    assert_eq!(count(&out2, "PageReference"), 1);
    assert!(well_formed(&out2));
}

#[test]
fn right_to_left_target_gets_direction_and_composer() {
    let langs = "\t<Language Self=\"Language/$ID/Arabic\" Name=\"$ID/Arabic\" Id=\"1\"/>\n\t<Language Self=\"Language/$ID/English%3a USA\" Name=\"$ID/English: USA\" Id=\"2\"/>\n";
    let body = psr_style(&csr_with("<Content>Hello world</Content>", NORMAL, " CharacterDirection=\"LeftToRightDirection\" AppliedLanguage=\"$ID/English: USA\""), "ParagraphStyle/Body");
    let data = icml(&body, langs, "");
    let out = translate_doc(&data, &[(0, "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}")], Some("ar"));
    assert_eq!(attr_vals(&out, "ParagraphStyleRange", "ParagraphDirection"), vec![Some("RightToLeftDirection".to_string())]);
    assert_eq!(attr_vals(&out, "ParagraphStyleRange", "Composer"), vec![Some("HL Composer Optyca".to_string())]);
    assert_eq!(attr_vals(&out, "CharacterStyleRange", "CharacterDirection"), vec![Some("DefaultDirection".to_string())]);
    assert_eq!(attr_vals(&out, "CharacterStyleRange", "AppliedLanguage"), vec![Some("$ID/Arabic".to_string())]);
    // a left-to-right target changes none of that
    let de = translate_doc(&data, &[(0, "Hallo Welt")], Some("de"));
    assert_eq!(attr_vals(&de, "ParagraphStyleRange", "ParagraphDirection"), vec![None]);
    assert_eq!(attr_vals(&de, "CharacterStyleRange", "CharacterDirection"), vec![Some("LeftToRightDirection".to_string())]);
}

#[test]
fn crlf_files_keep_their_line_ends() {
    let data: Vec<u8> = String::from_utf8(icml(&psr(&csr("<Content>First.</Content><Br/><Content>Second.</Content>")), "", "")).unwrap().replace('\n', "\r\n").into_bytes();
    let d = extract(&data);
    assert_eq!(Icml.merge(&d).unwrap(), data);
    let out = translate(&data, &[(1, "Zweiter.")]);
    assert_eq!(outside(&out, "u1"), outside(&data, "u1"));
    assert_eq!(units(&out), vec!["First.", "Zweiter."]);
    // the rewritten story keeps the untouched first paragraph's own bytes
    assert!(String::from_utf8(out).unwrap().contains("<Content>First.</Content>"));
}

#[test]
fn properties_are_copied_to_rebuilt_ranges() {
    let data = icml(
        &psr(&format!(
            "{}{}",
            csr_with("<Properties><AppliedFont type=\"string\">Minion</AppliedFont></Properties><Content>Plain </Content>", NORMAL, ""),
            csr_with("<Properties><AppliedFont type=\"string\">Arial</AppliedFont></Properties><Content>bold</Content>", BOLD, "")
        )),
        "",
        "",
    );
    let out = translate(&data, &[(0, "\u{27E6}r1\u{27E7}Fett\u{27E6}/r1\u{27E7} und normal")]);
    assert_eq!(count(&out, "Properties"), 2);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("Arial") && s.contains("Minion"), "{s}");
}

#[test]
fn several_segments_join_with_their_separators() {
    let data = icml(&psr(&csr("<Content>One. Two.</Content>")), "", "");
    let mut d = Icml.extract(&data, &opts()).unwrap();
    let u = &mut d.units[0];
    let mut a = u.segments[0].clone();
    a.id = "u0#1".into();
    a.source = vec![Inline::text("One.")];
    a.target = vec![Inline::text("Eins.")];
    let mut b = a.clone();
    b.id = "u0#2".into();
    b.source = vec![Inline::text("Two.")];
    b.target = vec![Inline::text("Zwei.")];
    u.segments = vec![a, b];
    u.separators = vec![" ".into()];
    let out = Icml.merge(&d).unwrap();
    assert_eq!(units(&out), vec!["Eins. Zwei."]);
}

#[test]
fn entities_and_char_refs_survive_untouched() {
    let data = icml(&psr(&csr("<Content>Fish &amp; chips &#x41; &#66; <![CDATA[x < y]]></Content>")), "", "");
    let d = extract(&data);
    assert_eq!(plain_text(&d.units[0].source()), "Fish & chips A B x < y");
    assert_eq!(Icml.merge(&d).unwrap(), data);
}

const SAMPLES: [(&str, &str); 11] = [
    ("en", "Welcome to the daily bread store"),
    ("de", "Willkommen im Brotladen, gr\u{fc}\u{df}e aus K\u{f6}ln"),
    ("fr", "Bienvenue \u{e0} la boulangerie, \u{e7}a sent bon"),
    ("es", "Bienvenido a la panader\u{ed}a, \u{bf}qu\u{e9} desea?"),
    ("pt", "Bem-vindo \u{e0} padaria, p\u{e3}o fresco"),
    ("ru", "\u{414}\u{43e}\u{431}\u{440}\u{43e} \u{43f}\u{43e}\u{436}\u{430}\u{43b}\u{43e}\u{432}\u{430}\u{442}\u{44c} \u{432} \u{43f}\u{435}\u{43a}\u{430}\u{440}\u{43d}\u{44e}"),
    ("tr", "F\u{131}r\u{131}na ho\u{15f} geldiniz, \u{11f}\u{fc}\u{15f}"),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{627}\u{644}\u{645}\u{62e}\u{628}\u{632}"),
    ("ja", "\u{30d1}\u{30f3}\u{5c4b}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}\u{3001}\u{713c}\u{304d}\u{305f}\u{3066}\u{3067}\u{3059}"),
    ("zh", "\u{6b22}\u{8fce}\u{6765}\u{5230}\u{9762}\u{5305}\u{5e97}\u{ff0c}\u{65b0}\u{9c9c}\u{51fa}\u{7089}"),
    ("ko", "\u{BE75}\u{C9D1}\u{C5D0} \u{C624}\u{C2E0} \u{AC83}\u{C744} \u{D658}\u{C601}\u{D569}\u{B2C8}\u{B2E4}"),
];

#[test]
fn eleven_languages_round_trip_and_stamp() {
    let langs = "\t<Language Self=\"Language/$ID/English%3a USA\" Name=\"$ID/English: USA\"/>\n\t<Language Self=\"L2\" Name=\"$ID/de_DE_2006\"/>\n\t<Language Self=\"L3\" Name=\"$ID/French\"/>\n\t<Language Self=\"L4\" Name=\"$ID/Spanish: Castilian\"/>\n\t<Language Self=\"L5\" Name=\"$ID/Portuguese: Brazilian\"/>\n\t<Language Self=\"L6\" Name=\"$ID/Russian\"/>\n\t<Language Self=\"L7\" Name=\"$ID/Turkish\"/>\n\t<Language Self=\"L8\" Name=\"$ID/Arabic\"/>\n\t<Language Self=\"L9\" Name=\"$ID/Japanese\"/>\n\t<Language Self=\"L10\" Name=\"$ID/Chinese: Simplified\"/>\n\t<Language Self=\"L11\" Name=\"$ID/Korean\"/>\n";
    let expect = ["$ID/English: USA", "$ID/de_DE_2006", "$ID/French", "$ID/Spanish: Castilian", "$ID/Portuguese: Brazilian", "$ID/Russian", "$ID/Turkish", "$ID/Arabic", "$ID/Japanese", "$ID/Chinese: Simplified", "$ID/Korean"];
    for (k, (lang, text)) in SAMPLES.iter().enumerate() {
        let body = psr(&format!("{}{}", csr(&format!("<Content>{text} </Content>")), csr_with("<Content>bold</Content>", BOLD, "")));
        let data = icml(&body, langs, "");
        let d = extract(&data);
        assert_eq!(plain_text(&d.units[0].source()), format!("{text} bold"), "{lang}");
        assert_eq!(Icml.merge(&d).unwrap(), data, "{lang} untouched");
        for (l2, t2) in SAMPLES {
            let target = if l2 == "pt" { "pt-BR" } else { l2 };
            let out = translate_doc(&data, &[(0, &format!("{t2} \u{27E6}r1\u{27E7}bold\u{27E6}/r1\u{27E7}"))], Some(target));
            assert!(well_formed(&out), "{lang}->{l2}");
            assert!(visible_text(&out).contains(t2), "{lang}->{l2}");
            let want = expect[SAMPLES.iter().position(|(l, _)| *l == l2).unwrap()];
            assert!(attr_vals(&out, "CharacterStyleRange", "AppliedLanguage").iter().all(|a| a.as_deref() == Some(want)), "{lang}->{l2}");
            let rtl = attr_vals(&out, "ParagraphStyleRange", "ParagraphDirection") == vec![Some("RightToLeftDirection".to_string())];
            assert_eq!(rtl, l2 == "ar", "{lang}->{l2} direction");
            assert_eq!(extract(&out).units.len(), 1);
        }
        let _ = k;
    }
}

#[test]
fn only_utf8_is_read() {
    let data = icml(&psr(&csr("<Content>caf\u{e9}</Content>")), "", "");
    let latin1: Vec<u8> = String::from_utf8(data.clone()).unwrap().replace("encoding=\"UTF-8\"", "encoding=\"ISO-8859-1\"").chars().map(|c| c as u32 as u8).collect();
    assert!(matches!(Icml.extract(&latin1, &opts()), Err(Error::Encoding(_))), "declared Latin-1");
    let undeclared: Vec<u8> = String::from_utf8(data.clone()).unwrap().replace(" encoding=\"UTF-8\"", "").chars().map(|c| c as u32 as u8).collect();
    assert!(matches!(Icml.extract(&undeclared, &opts()), Err(Error::Encoding(_))), "undeclared, not UTF-8");
    let sjis = String::from_utf8(data.clone()).unwrap().replace("UTF-8", "Shift_JIS");
    assert!(matches!(Icml.extract(sjis.as_bytes(), &opts()), Err(Error::Encoding(_))), "declared Shift_JIS");
    // "utf8" and lower case are fine
    let ok = String::from_utf8(data.clone()).unwrap().replace("UTF-8", "utf8");
    assert_eq!(Icml.extract(ok.as_bytes(), &opts()).unwrap().units.len(), 1);
    // a doubled BOM is not XML
    let mut dbl = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    dbl.extend_from_slice(&data);
    assert!(matches!(Icml.extract(&dbl, &opts()), Err(Error::Parse { .. })));
    let _ = open("x");
}

#[test]
fn stale_skeleton_and_foreign_document_are_refused() {
    let data = icml(&psr(&csr("<Content>Hello</Content>")), "", "");
    let mut d = Icml.extract(&data, &opts()).unwrap();
    d.units[0].segments[0].target = vec![Inline::text("Hallo")];
    let Skeleton::Text { parser, .. } = &mut d.skeleton else { panic!() };
    *parser = Some(ParserInfo::new("bilingua-other", "0.1.0", 1));
    assert!(matches!(Icml.merge(&d), Err(Error::ParserVersion { .. })));
    let mut d = Icml.extract(&data, &opts()).unwrap();
    d.units[0].segments[0].target = vec![Inline::text("Hallo")];
    d.units[0].codes.insert("~p".into(), CodeData { standalone: Some("99".into()), ..Default::default() });
    assert!(matches!(Icml.merge(&d), Err(Error::Write(_))));
}
