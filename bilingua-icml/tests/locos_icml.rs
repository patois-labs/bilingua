//! The LocOS suite `tests/verify_icml_filter.py`, ported: 12 cases (11 here; the twelfth, the four-surface
//! upload/translate/export test, is covered by `surfaces_translate_and_export` and, for the web/API/CLI/MCP doors,
//! belongs to the LocOS adapter).

mod common;

use bilingua_icml::Icml;
use bilingua_model::*;
use common::*;

fn refused(data: &[u8], why: &str) {
    assert!(Icml.extract(data, &opts()).is_err(), "{why}: extract must refuse");
}

fn mapping_of(us: &[Unit], i: usize) -> usize {
    p_index(&us[i])
}

#[test]
fn registered_by_extension_and_sniff() {
    // LocOS test_registered_everywhere: the format and its extensions (the web/API/MCP surfaces are LocOS's).
    assert_eq!(Icml.id(), "icml");
    assert!(Icml.extensions().contains(&"icml") && Icml.extensions().contains(&"wcml"));
    assert!(Icml.sniff(&icml(&psr(&csr("<Content>x</Content>")), "", "")));
    assert!(!Icml.sniff(b"<html><body>hi</body></html>"));
    assert!(!Icml.sniff(b"plain text"));
}

#[test]
fn identity_and_untouched_bytes() {
    let hyper = "\t<Hyperlink Self=\"hl1\" Name=\"site\" Source=\"h1\" Visible=\"false\"><Properties><Destination type=\"object\">HyperlinkURLDestination/x</Destination></Properties></Hyperlink>\n";
    let body = psr(&csr("<Content>First paragraph.</Content><Br/><Content>Second paragraph.</Content>"));
    let data = icml(&body, "", hyper);
    let d = extract(&data);
    let us = &d.units;
    assert_eq!(units(&data), vec!["First paragraph.", "Second paragraph."]);
    assert_eq!(Icml.merge(&d).unwrap(), data, "no translation must return the exact bytes");
    // every unit echoed back
    let echo: Vec<(usize, String)> = us.iter().map(|u| (p_index(u), tok_text(u, &u.source()))).collect();
    let m: Vec<(usize, &str)> = echo.iter().map(|(i, s)| (*i, s.as_str())).collect();
    let same = translate(&data, &m);
    assert_eq!(units(&same), units(&data));
    let out = translate(&data, &[(mapping_of(us, 1), "Zweiter Absatz, viel laenger.")]);
    assert_eq!(outside(&out, "u1"), outside(&data, "u1"), "bytes outside the story must not change");
    let head = &data[..data.windows(9).position(|w| w == b"<Document").unwrap()];
    assert!(out.starts_with(head), "declaration + aid PIs kept");
    assert_eq!(units(&out), vec!["First paragraph.", "Zweiter Absatz, viel laenger."]);
    // BOM kept
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(&data);
    let out = translate(&bom, &[(mapping_of(us, 0), "Erster.")]);
    assert!(out.starts_with(b"\xEF\xBB\xBF<?xml"), "{:?}", &out[..20]);
    assert_eq!(units(&out)[0], "Erster.");
}

#[test]
fn nested_styles_one_segment_and_growth() {
    let body = psr(&format!("{}\n{}\n{}", csr("<Content>Hello </Content>"), csr_with("<Content>bold</Content>", BOLD, ""), csr("<Content> world</Content>")));
    let data = icml(&body, "", "");
    let us = units(&data);
    assert_eq!(us, vec!["Hello \u{27E6}r1\u{27E7}bold\u{27E6}/r1\u{27E7} world"]);
    let new = "\u{27E6}r1\u{27E7}Fettgedruckt\u{27E6}/r1\u{27E7} ist hier, hallo liebe Welt";
    let out = translate(&data, &[(0, new)]);
    let got = story_ranges(&out);
    assert_eq!(got, vec![(BOLD.to_string(), "Fettgedruckt".to_string()), (NORMAL.to_string(), "ist hier, hallo liebe Welt".to_string())]);
    assert_eq!(units(&out)[0], new);
}

fn specials_doc() -> Vec<u8> {
    let footnote = format!("<Footnote>{}</Footnote>", psr(&csr("<Content><?ACE 4?> Footnote text.</Content>")));
    let table = format!(
        "<Table Self=\"t1\" BodyRowCount=\"1\" ColumnCount=\"2\"><Row Self=\"t1r0\" Name=\"0\"/><Column Self=\"t1c0\" Name=\"0\"/><Column Self=\"t1c1\" Name=\"1\"/><Cell Self=\"t1i0\" Name=\"0:0\">{}</Cell><Cell Self=\"t1i1\" Name=\"1:0\">{}</Cell></Table>",
        psr(&csr("<Content>Cell A</Content>")),
        psr(&csr("<Content>Cell B</Content>"))
    );
    let body1 = psr(&csr(&format!(
        "<Content>Page\t<?ACE 18?> see </Content><HyperlinkTextSource Self=\"h1\" Name=\"link\"><Content>our site</Content></HyperlinkTextSource><Content> now</Content><PageReference Self=\"p1\" PageReferenceType=\"CurrentPage\"/><Content>.</Content>{footnote}"
    )));
    let body = format!("{body1}\n{}", psr(&csr(&table)));
    icml(&body, "", "")
}

#[test]
fn specials_footnote_table_hyperlink() {
    let data = specials_doc();
    let d = extract(&data);
    let texts = units(&data);
    assert_eq!(
        texts,
        vec![
            "Page\u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7} see \u{27E6}g3\u{27E7}our site\u{27E6}/g3\u{27E7} now\u{27E6}x4\u{27E7}.\u{27E6}x5\u{27E7}",
            "\u{27E6}x1\u{27E7} Footnote text.",
            "Cell A",
            "Cell B"
        ]
    );
    let new = [
        "Seite\u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7} siehe \u{27E6}g3\u{27E7}unsere Seite\u{27E6}/g3\u{27E7} jetzt\u{27E6}x4\u{27E7}.\u{27E6}x5\u{27E7}",
        "\u{27E6}x1\u{27E7} Fussnote.",
        "Zelle A",
        "Zelle B",
    ];
    let m: Vec<(usize, &str)> = d.units.iter().zip(new.iter()).map(|(u, t)| (p_index(u), *t)).collect();
    let out = translate(&data, &m);
    let s = String::from_utf8(out.clone()).unwrap();
    assert!(s.contains("<Content>Seite\t<?ACE 18?> siehe </Content>"), "{s}");
    assert_eq!(count(&out, "HyperlinkTextSource"), 1);
    assert_eq!(attr_vals(&out, "HyperlinkTextSource", "Self"), vec![Some("h1".to_string())]);
    assert_eq!(count(&out, "PageReference"), 1);
    assert_eq!(count(&out, "Footnote"), 1);
    assert_eq!(count(&out, "Cell"), 2);
    assert!(visible_text(&out).contains("unsere Seite"));
    assert_eq!(units(&out), new.to_vec());
}

#[test]
fn hidden_condition_in_same_file_and_notes() {
    let conds = "\t<Condition Self=\"Condition/Print Only\" Name=\"Print Only\" Visible=\"false\"/>\n";
    let body = psr(&format!(
        "{}{}{}",
        csr("<Content>Visible </Content>"),
        csr_with("<Content>SECRET</Content>", NORMAL, " AppliedConditions=\"Condition/Print%20Only\""),
        csr("<Content> tail</Content><Note><ParagraphStyleRange><CharacterStyleRange><Content>editor note</Content></CharacterStyleRange></ParagraphStyleRange></Note>")
    ));
    let data = icml(&body, conds, "");
    let d = extract(&data);
    assert_eq!(d.units.len(), 1);
    let t = tok_text(&d.units[0], &d.units[0].source());
    assert!(!t.contains("SECRET") && !t.contains("editor note"), "{t}");
    let out = translate(&data, &[(0, &t.replace("Visible", "Sichtbar"))]);
    let txt = visible_text(&out);
    assert!(txt.contains("SECRET") && txt.contains("editor note") && txt.contains("Sichtbar"), "{txt}");
    assert!(well_formed(&out));
}

#[test]
fn target_language_stamped() {
    let langs = "\t<Language Self=\"Language/$ID/English%3a USA\" Name=\"$ID/English: USA\" Id=\"269\"/>\n\t<Language Self=\"Language/$ID/de_DE_2006\" Name=\"$ID/de_DE_2006\" Id=\"275\"/>\n";
    let body = psr(&csr_with("<Content>Hello</Content>", NORMAL, " AppliedLanguage=\"$ID/English: USA\""));
    let data = icml(&body, langs, "");
    let lang_of = |out: &[u8]| attr_vals(out, "CharacterStyleRange", "AppliedLanguage")[0].clone();
    let plain = translate(&data, &[(0, "Hallo")]);
    assert_eq!(lang_of(&plain).as_deref(), Some("$ID/English: USA"));
    let de = translate_doc(&data, &[(0, "Hallo")], Some("de-DE"));
    assert_eq!(lang_of(&de).as_deref(), Some("$ID/de_DE_2006"));
    let fr = translate_doc(&data, &[(0, "Hallo")], Some("fr-FR"));
    assert_eq!(lang_of(&fr).as_deref(), Some("$ID/English: USA"), "an undeclared language is never invented");
}

#[test]
fn wcml_multi_story_article() {
    let ns = "urn:SmartConnection_v3";
    let data = format!(
        "{}\t<ea:Stories>\n\t<ea:Story ea:GUID=\"g1\"><ea:StoryInfo><ea:SI_EL>head</ea:SI_EL></ea:StoryInfo>\n{}\t</ea:Story>\n\t<ea:Story ea:GUID=\"g2\"><ea:StoryInfo><ea:SI_EL>body</ea:SI_EL></ea:StoryInfo>\n{}\t</ea:Story>\n\t</ea:Stories>\n</Document>\n",
        HEAD.replace("<Document DOMVersion", &format!("<Document xmlns:ea=\"{ns}\" DOMVersion")),
        story("s1", &psr(&csr("<Content>Headline here</Content>"))),
        story("s2", &psr(&csr("<Content>Body text.</Content>")))
    )
    .into_bytes();
    assert_eq!(units(&data), vec!["Headline here", "Body text."]);
    let out = translate(&data, &[(1, "Fliesstext.")]);
    assert_eq!(outside(&out, "s2"), outside(&data, "s2"), "headline story and wrappers byte-identical");
    assert_eq!(units(&out), vec!["Headline here", "Fliesstext."]);
}

fn snippet(sid: &str, body: &str, resources: &str) -> String {
    format!(
        "\t<Document DOMVersion=\"8.0\" Self=\"d\">\n\t<Color Self=\"Color/Black\" Model=\"Process\" Space=\"CMYK\" ColorValue=\"0 0 0 100\" Name=\"Black\"/>\n{resources}{}\t</Document>\n",
        story(sid, body)
    )
}

fn smart_connection_v3() -> Vec<u8> {
    let ns = "urn:SmartConnection_v3";
    let hidden = "\t<Condition Self=\"Condition/Draft\" Name=\"Draft\" Visible=\"false\"/>\n";
    let shown = "\t<Condition Self=\"Condition/Draft\" Name=\"Draft\" Visible=\"true\"/>\n";
    let draft = " AppliedConditions=\"Condition/Draft\"";
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?>\n\
<ea:Stories ea:WWVersion=\"2.0\" xmlns:aic=\"http://ns.adobe.com/AdobeInCopy/2.0\" xmlns:ea=\"{ns}\">\n\
\t<ea:Story ea:GUID=\"g1\" pi0=\"style=&quot;50&quot; type=&quot;snippet&quot;\" pi1=\"SnippetType=&quot;InCopyInterchange&quot;\">\n\
\t\t<ea:StoryInfo><ea:SI_EL>head</ea:SI_EL><ea:SI_Snippet>Headline preview</ea:SI_Snippet></ea:StoryInfo>\n\
{}\t</ea:Story>\n\
\t<ea:Story ea:GUID=\"g2\" pi0=\"\" pi1=\"\">\n\
\t\t<ea:StoryInfo><ea:SI_EL>body</ea:SI_EL></ea:StoryInfo>\n\
{}\t</ea:Story>\n\
\t<ea:wwsd_document><ea:wwsd_styles>\n\
\t<Document DOMVersion=\"8.0\" Self=\"d\"><Color Self=\"Color/Black\" Name=\"Black\"/></Document>\n\
\t</ea:wwsd_styles></ea:wwsd_document>\n\
</ea:Stories>\n",
        snippet("s1", &psr(&format!("{}{}", csr("<Content>Headline here</Content>"), csr_with("<Content> DRAFT</Content>", NORMAL, draft))), hidden),
        snippet("s2", &psr(&csr_with("<Content>Body text.</Content>", NORMAL, draft)), shown)
    )
    .into_bytes()
}

#[test]
fn wcml_smart_connection_v3_root() {
    let data = smart_connection_v3();
    let d = extract(&data);
    assert_eq!(Icml.merge(&d).unwrap(), data, "no-change build is byte-identical");
    // The first snippet hides Draft; the second declares Draft visible: each story honours only its own
    // document's conditions. StoryInfo metadata is never offered.
    assert_eq!(units(&data), vec!["Headline here", "Body text."]);
    let out = translate(&data, &[(1, "Fliesstext.")]);
    assert_eq!(outside(&out, "s2"), outside(&data, "s2"), "wrapper and headline story byte-identical");
    let cut = data.windows(16).position(|w| w == b"<Story Self=\"s2\"").unwrap();
    assert!(out.starts_with(&data[..cut]), "ea:Story wrapper kept");
    assert_eq!(units(&out), vec!["Headline here", "Fliesstext."]);
    assert!(out.windows(16).any(|w| w == b"Headline preview") && out.windows(5).any(|w| w == b"DRAFT"));
    let loc = translate_doc(&data, &[(0, "Titel hier")], Some("de-DE"));
    assert_eq!(units(&loc), vec!["Titel hier", "Body text."]);
    // Only WoodWing's own namespace opens; another <Stories> root is still refused.
    let foreign = String::from_utf8(data.clone()).unwrap().replace("urn:SmartConnection_v3", "urn:example").into_bytes();
    refused(&foreign, "foreign Stories root");
    // A wrapper with no snippet story inside a <Document> holds nothing to translate.
    refused(
        b"<ea:Stories xmlns:ea=\"urn:SmartConnection_v3\"><ea:Story><Story Self=\"x\"/></ea:Story></ea:Stories>",
        "story outside a snippet document",
    );
}

#[test]
fn unlocatable_story_comment_still_splices_exactly() {
    // LocOS falls back to a whole-document re-serialization when a comment mentions a story tag; the span-exact
    // writer needs no fallback, and keeps the declaration, the aid PIs and the comment.
    let data = icml(&psr(&csr("<Content>Hello.</Content>")), "", "\t<!-- old <Story> removed -->\n");
    let out = translate(&data, &[(0, "Hallo.")]);
    assert!(out.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>"));
    assert!(out.windows(34).any(|w| w == b"<?aid SnippetType=\"InCopyInterchan"));
    assert!(out.windows(27).any(|w| w == b"<!-- old <Story> removed --"));
    assert_eq!(units(&out)[0], "Hallo.");
    assert_eq!(outside(&out, "u1"), outside(&data, "u1"));
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn damaged_tokens_fuzz() {
    let mut rng = Rng(1010);
    let body = psr(&format!(
        "{}{}{}",
        csr("<Content>Start\t</Content><HyperlinkTextSource Self=\"hz\"><Content>link</Content></HyperlinkTextSource><PageReference Self=\"pz\"/><Content>mid<?ACE 7?>dle</Content>"),
        csr_with("<Content> bold </Content>", BOLD, ""),
        csr("<Content> end.</Content><Br/><Content>Next para.</Content>")
    ));
    let data = icml(&body, "", "");
    let first = &extract(&data).units[0];
    let src = tok_text(first, &first.source());
    let toks: Vec<String> = src.split('\u{27E6}').skip(1).map(|t| format!("\u{27E6}{}", t.split('\u{27E7}').next().unwrap_or("")) + "\u{27E7}").collect();
    assert!(toks.len() >= 6, "{src}");
    for _ in 0..150 {
        let mut picked: Vec<String> = toks.iter().filter(|_| rng.below(10) >= 2).cloned().collect();
        for _ in 0..rng.below(3) {
            picked.push(toks[rng.below(toks.len())].clone());
        }
        for i in (1..picked.len()).rev() {
            picked.swap(i, rng.below(i + 1));
        }
        let words = ["Wort", "zwei", " ", "drei"];
        let text: String = picked.iter().map(|t| format!("{t}{}", words[rng.below(4)])).collect();
        let out = translate(&data, &[(0, &text)]);
        assert!(well_formed(&out), "{text}");
        assert_eq!(attr_vals(&out, "HyperlinkTextSource", "Self"), vec![Some("hz".to_string())], "{text}");
        assert_eq!(attr_vals(&out, "PageReference", "Self"), vec![Some("pz".to_string())], "{text}");
        assert_eq!(count(&out, "Br"), 1, "{text}");
        assert!(!visible_text(&out).contains('\u{27E6}'), "{text}");
        assert_eq!(count(&out, "RootCharacterStyleGroup"), 1);
        assert!(!extract(&out).units.is_empty());
    }
}

#[test]
fn malformed_and_hostile() {
    refused(b"not xml at all", "not xml");
    refused(b"PK\x03\x04 a zip renamed", "zip");
    refused(b"<?xml version=\"1.0\"?><Root><Story/></Root>", "wrong root");
    refused(b"<?xml version=\"1.0\"?><Document Self=\"d\"/>", "no story");
    refused(
        b"<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/passwd\">]><Document><Story><ParagraphStyleRange><CharacterStyleRange><Content>&e;</Content></CharacterStyleRange></ParagraphStyleRange></Story></Document>",
        "XXE",
    );
    refused(
        b"<?xml version=\"1.0\"?><!DOCTYPE l [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;&a;\">]><Document><Story/></Document>",
        "billion laughs",
    );
    refused(b"<Document><Story><ParagraphStyleRange>", "truncated");
    let mut utf16 = vec![0xFF, 0xFE];
    for c in "<?xml version=\"1.0\" encoding=\"UTF-16\"?><Document><Story/></Document>".encode_utf16() {
        utf16.extend_from_slice(&c.to_le_bytes());
    }
    refused(&utf16, "UTF-16");
    // markup budget (forty `<` per allowed unit): refused before a tree is built
    let small = Limits::new().with_max_units(2);
    let big = icml(&psr(&csr(&"<Content>x</Content>".repeat(100))), "", "");
    match Icml.extract_with(&big, &opts(), &small) {
        Err(Error::Limit(_)) => {}
        other => panic!("markup budget must be a limit error: {other:?}"),
    }
    let many = icml(&psr(&csr(&(0..4).map(|i| format!("<Content>p{i}</Content>")).collect::<Vec<_>>().join("<Br/>"))), "", "");
    let cap = Limits::new().with_max_units(3);
    assert!(many.iter().filter(|c| **c == b'<').count() < 120);
    match Icml.extract_with(&many, &opts(), &cap) {
        Err(Error::Limit(e)) => assert!(e.msg.to_lowercase().contains("unit") || e.msg.to_lowercase().contains("segment"), "{}", e.msg),
        other => panic!("segment cap must be a limit error: {other:?}"),
    }
}

#[test]
fn surfaces_translate_and_export() {
    // The LocOS four-surface test, minus the doors: a mock engine prefixes "[de-DE] " to every unit and the export
    // keeps every tag.
    let data = icml(
        &psr(&format!(
            "{}{}{}",
            csr("<Content>Hello </Content>"),
            csr_with("<Content>bold</Content>", BOLD, ""),
            csr("<Content> world.</Content><Br/><Content>Second\tline.</Content>")
        )),
        "",
        "",
    );
    let mut d = Icml.extract(&data, &opts_to("de-DE")).unwrap();
    for u in &mut d.units {
        let mut t = vec![Inline::text("[de-DE] ")];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out = Icml.merge(&d).unwrap();
    assert_eq!(outside(&out, "u1"), outside(&data, "u1"));
    let txt = visible_text(&out);
    assert!(txt.contains("[de-DE]") && !txt.contains('\u{27E6}') && txt.contains('\t'), "{txt}");
    assert!(story_ranges(&out).iter().any(|(s, _)| s == BOLD));
    assert!(well_formed(&out));
    // junk is refused
    refused(b"<Document><Story>", "junk");
}

/// W4 icml-2: a rebuilt paragraph in a CRLF file keeps CRLF everywhere (LocOS `newline_like`); an LF file stays LF.
#[test]
fn crlf_file_stays_crlf_after_a_rebuild() {
    let body = psr(&format!("{}\n{}", csr("<Content>Plain </Content>"), csr_with("<Content>bold</Content>", BOLD, "")));
    let lf = icml(&body, "", "");
    let crlf = String::from_utf8(lf.clone()).unwrap().replace('\n', "\r\n").into_bytes();
    for (data, want_crlf) in [(crlf, true), (lf, false)] {
        let us = units(&data);
        assert_eq!(us, vec!["Plain \u{27E6}r1\u{27E7}bold\u{27E6}/r1\u{27E7}"]);
        let out = translate(&data, &[(0, "\u{27E6}r1\u{27E7}fett\u{27E6}/r1\u{27E7} schlicht")]);
        assert!(well_formed(&out));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("fett") && s.contains("schlicht"), "{s}");
        let bare_lf = s.as_bytes().iter().enumerate().filter(|(i, c)| **c == b'\n' && (*i == 0 || s.as_bytes()[i - 1] != b'\r')).count();
        let crs = s.matches('\r').count();
        if want_crlf {
            assert_eq!(bare_lf, 0, "a bare LF in a CRLF file:\n{s:?}");
        } else {
            assert_eq!(crs, 0, "a CR in an LF file:\n{s:?}");
        }
    }
}

/// W4 icml-3: a bare `<Story>` root is read natively (as Okapi does) but refused under the LocOS profile, as LocOS
/// refuses it.
#[test]
fn bare_story_root_refused_under_the_locos_profile() {
    let data = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", story("u1", &psr(&csr("<Content>Alone.</Content>")))).into_bytes();
    assert_eq!(units(&data), vec!["Alone."]);
    let err = Icml.extract(&data, &profile::locos().extract_options()).unwrap_err();
    assert!(err.to_string().contains("its root is neither <Document> nor WoodWing"), "{err}");
    // a normal file is still read under the profile
    assert!(Icml.extract(&icml(&psr(&csr("<Content>Fine.</Content>")), "", ""), &profile::locos().extract_options()).is_ok());
}

/// W4 icml-4: a table written straight into the story (pandoc) is read natively, but not under the LocOS profile
/// (LocOS's walker reads tables only inside a range), so the paragraphs after it keep LocOS's numbers; a profile
/// merge walks the same way.
#[test]
fn story_level_table_is_skipped_under_the_locos_profile() {
    let cell = |n: &str, t: &str| format!("<Cell Name=\"{n}\">{}</Cell>", psr(&csr(&format!("<Content>{t}</Content>"))));
    let body = format!(
        "{}\n<Table ColumnCount=\"2\">{}{}</Table>\n{}",
        psr(&csr("<Content>Before.</Content>")),
        cell("0:0", "Head"),
        cell("1:0", "Cell"),
        psr(&csr("<Content>After.</Content>"))
    );
    let data = icml(&body, "", "");
    assert_eq!(units(&data), vec!["Before.", "After.", "Head", "Cell"]);
    let lo = profile::locos().extract_options().with_granularity(Granularity::Paragraph);
    let mut d = Icml.extract(&data, &lo).unwrap();
    let got: Vec<(usize, String)> = d.units.iter().map(|u| (p_index(u), tok_text(u, &u.source()))).collect();
    assert_eq!(got, vec![(0, "Before.".to_string()), (1, "After.".to_string())]);
    d.units[1].segments[0].target = vec![Inline::text("Danach.")];
    let out = Icml.merge_with(&d, &profile::locos().merge_options("icml")).unwrap();
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("Danach.") && s.contains("<Content>Head</Content>") && s.contains("<Content>Before.</Content>"), "{s}");
}
