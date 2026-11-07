//! Opus gate cases: the MadCap mask round trip under attack (marker look-alikes in the text and in translations,
//! nested, unclosed and orphan MadCap elements, conditional text, snippets, variables, xrefs, keywords, popups),
//! `<title>` handling, injection through translations into every part type, empty translations and merge options.
//! Expected unit texts are LocOS's (`flare_filter`, run read-only on the same topics) unless a comment says why not.
mod common;
use bilingua_flare::Flare;
use bilingua_model::*;
use common::*;

const NS: &str = "xmlns:MadCap=\"http://www.madcapsoftware.com/Schemas/MadCap.xsd\"";

fn topic(body: &str, head: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html {NS}><head>{head}</head><body>{body}</body></html>\n")
}

fn proj(t: &str) -> Vec<u8> {
    make_zip(&[("Help.flprj".into(), b"<?xml version=\"1.0\"?><CatapultProject/>".to_vec()), ("Content/T.htm".into(), t.as_bytes().to_vec())])
}

fn out_topic(b: &[u8]) -> String {
    let (m, _) = members(b);
    text_of(&m, "Content/T.htm")
}

/// No mask of this crate survives in `s` as markup.
fn no_mask_markup(s: &str) {
    for bad in ["<span mcf", "</span mcf", "<wbr mcf"] {
        assert!(!s.contains(bad), "a mask leaked as markup: {s}");
    }
}

fn count(s: &str, pat: &str) -> usize {
    s.matches(pat).count()
}

#[test]
fn madcap_inline_elements_match_locos_and_round_trip() {
    let t = |b: &str| topic(b, "<title>T</title>");
    // (topic body, LocOS units after the title, well-formed source)
    let cases: Vec<(String, Vec<&str>, bool)> = vec![
        (
            t("<p>A <MadCap:xref href=\"x\">B <MadCap:conditionalText MadCap:conditions=\"c\">C</MadCap:conditionalText> D</MadCap:xref> E</p>"),
            vec!["A \u{27E6}g1\u{27E7}B \u{27E6}g2\u{27E7}C\u{27E6}/g2\u{27E7} D\u{27E6}/g1\u{27E7} E"],
            true,
        ),
        (t("<p>Hi <MadCap:variable name=\"a\">x <b>y</b></MadCap:variable> there</p>"), vec!["Hi \u{27E6}x1\u{27E7} there"], true),
        (t("<p>K <MadCap:keyword term=\"X\"></MadCap:keyword> end</p>"), vec!["K \u{27E6}x1\u{27E7}\u{27E6}x2\u{27E7} end", "X"], true),
        (t("<p>A <MadCap:concept term=\"c1\" /> B <MadCap:snippetText src=\"s.flsnp\"/> C</p>"), vec!["A \u{27E6}x1\u{27E7} B \u{27E6}x2\u{27E7} C"], true),
        (
            t("<p>See <MadCap:xref href=\"a.htm\">the <MadCap:variable name=\"v.w\" /> guide</MadCap:xref>.</p>"),
            vec!["See \u{27E6}g1\u{27E7}the \u{27E6}x2\u{27E7} guide\u{27E6}/g1\u{27E7}."],
            true,
        ),
        (
            t("<p>A <MadCap:textPopup><MadCap:textPopupHead>Head</MadCap:textPopupHead><MadCap:textPopupBody>Body</MadCap:textPopupBody></MadCap:textPopup> z</p>"),
            vec!["A \u{27E6}g1\u{27E7}\u{27E6}g2\u{27E7}Head\u{27E6}/g2\u{27E7}\u{27E6}g3\u{27E7}Body\u{27E6}/g3\u{27E7}\u{27E6}/g1\u{27E7} z"],
            true,
        ),
        (t("<p>A <MadCap:xref href=\"x\" /> B</p>"), vec!["A \u{27E6}x1\u{27E7} B"], true),
        (t("<p>K<MadCap:keyword term=\"A &amp; B;C\\:D\" /></p>"), vec!["K\u{27E6}x1\u{27E7}", "A & B", "C:D"], true),
        // text that looks like a mask, and the marker name in the text
        (t("<p>Use &lt;span mcf=\"0\"&gt; and mcfxxxxxxx here <MadCap:xref href=\"x\">B</MadCap:xref></p>"), vec!["Use &lt;span mcf=\"0\"&gt; and mcfxxxxxxx here \u{27E6}g1\u{27E7}B\u{27E6}/g1\u{27E7}"], true),
        // unclosed (LocOS: an open token with no close, `⟦g1⟧B`; the mask makes it a point, so the markup stays
        // where it was)
        (t("<p>A <MadCap:xref href=\"x\">B</p><p>C</p>"), vec!["A \u{27E6}x1\u{27E7}B", "C"], false),
    ];
    for (src_topic, want, wf) in cases {
        let src = proj(&src_topic);
        let mut d = extract(&src);
        let got = loc_texts(&d);
        assert_eq!(got[0], "T");
        assert_eq!(&got[1..], want.as_slice(), "{src_topic}");
        assert_eq!(merge(&d), src, "untouched: {src_topic}");
        translate_all(&mut d);
        let out = out_topic(&merge(&d));
        no_mask_markup(&out);
        assert_eq!(count(&out, "<MadCap:"), count(&src_topic, "<MadCap:"), "{out}");
        assert_eq!(count(&out, "</MadCap:"), count(&src_topic, "</MadCap:"), "{out}");
        if wf {
            let (m, _) = members(&merge(&d));
            well_formed(&m);
        }
        // the translated project reads again with the same units, translated
        let again = extract(&merge(&d));
        assert_eq!(again.units.len(), d.units.len(), "{out}");
    }
    // an orphan end tag is a point code (LocOS issue 810, kept on purpose): LocOS drops it from the unit text and
    // from a translated build; here an untouched and a translated merge both keep it
    let src_topic = t("<p>A </MadCap:xref> B</p>");
    let src = proj(&src_topic);
    let mut d = extract(&src);
    assert_eq!(&loc_texts(&d)[1..], ["A \u{27E6}x1\u{27E7} B"]);
    assert_eq!(merge(&d), src);
    translate_all(&mut d);
    assert_eq!(count(&out_topic(&merge(&d)), "</MadCap:"), 1);
}

#[test]
fn marker_look_alikes_in_translations_stay_text_in_every_slot() {
    let t = topic(
        "<p>A <MadCap:xref href=\"x\">B</MadCap:xref> <MadCap:variable name=\"v\" /> C</p><p><img src=\"a.png\" alt='Logo' /> x</p><p>K<MadCap:keyword term=\"Alpha\" /></p>",
        "<title>T</title>",
    );
    let src = proj(&t);
    let mut d = extract(&src);
    for i in 0..d.units.len() {
        let lt = loc_text(&d.units[i]);
        set(&mut d, i, &format!("<span mcf=\"0\">x</span mcf=\"0\"><wbr mcf=\"1\"/> mcf=\"0\" mcfx=\"1\" ' \" ]]> &nbsp; <!-- <MadCap:xref href=\"evil\"> {lt}"));
    }
    let b = merge(&d);
    let out = out_topic(&b);
    no_mask_markup(&out);
    let (m, _) = members(&b);
    well_formed(&m);
    // the original MadCap elements only, once each
    assert_eq!(count(&out, "<MadCap:xref href=\"x\">"), 1, "{out}");
    assert_eq!(count(&out, "<MadCap:xref"), 1, "{out}");
    assert_eq!(count(&out, "<MadCap:variable name=\"v\" />"), 1, "{out}");
    // the value is written inside the source's own quotes (LocOS `_apply_attr_units`)
    assert!(out.contains("alt='&lt;span mcf=&quot;0&quot;&gt;"), "{out}");
    assert!(out.contains("<MadCap:keyword term=\"&lt;span mcf=&quot;0&quot;&gt;"), "{out}");
    assert!(!out.contains("<!--"), "{out}");
}

#[test]
fn a_title_with_markup_is_offered_with_codes_as_locos_does() {
    // LocOS offers `Guide ⟦x1⟧ now` (since D4 the HTML filter reads <title> as LocOS does, markup as codes)
    let t = topic("<p>x</p>", "<title>Guide <MadCap:variable name=\"a.b\" /> now</title>");
    let src = proj(&t);
    let mut d = extract(&src);
    assert_eq!(loc_texts(&d), ["Guide \u{27E6}x1\u{27E7} now", "x"]);
    assert_eq!(merge(&d), src);
    set(&mut d, 0, "Leitfaden \u{27E6}x1\u{27E7} jetzt");
    assert!(out_topic(&merge(&d)).contains("<title>Leitfaden <MadCap:variable name=\"a.b\" /> jetzt</title><"));
    // `&lt;` is text: the title is offered (LocOS: `A &lt; B`, the reference as typed) and written escaped
    let t = topic("<p>x</p>", "<title>A &lt; B</title>");
    let mut d = extract(&proj(&t));
    assert_eq!(loc_texts(&d), ["A &lt; B", "x"]);
    assert!(d.units[0].translate);
    set(&mut d, 0, "B < A & C");
    assert!(out_topic(&merge(&d)).contains("<title>B &lt; A &amp; C</title>"));
}

#[test]
fn private_use_characters_in_the_topic_survive_a_merge() {
    // a topic that holds the whole BMP private-use area merges without touching any of them (the `&` sentinel of
    // the old HTML filter is gone since D4)
    let pua: String = ('\u{E000}'..='\u{F8FF}').collect();
    let t = topic(&format!("<p>{pua}</p><p>Fish &amp; chips</p>"), "<title>T</title>");
    let mut d = extract(&proj(&t));
    let i = index_of(&d, "Fish &amp; chips");
    set(&mut d, i, "Poisson & frites &nbsp;");
    let out = out_topic(&merge(&d));
    assert!(out.contains(&pua), "a private-use character was rewritten");
    // LocOS: a body target is markup text, `&nbsp;` written XML-safe
    assert!(out.contains("Poisson &amp; frites &#160;"), "{out}");
}

#[test]
fn translations_cannot_inject_markup_into_structured_parts() {
    let src = project();
    let mut d = extract(&src);
    let evil = "x\" Link=\"http://evil/\" a='b' </Variable><Variable Name=\"Q\">q</Variable> ]]> <![CDATA[ &amp; &bogus; \u{1}";
    for i in 0..d.units.len() {
        if d.units[i].translate {
            set(&mut d, i, evil);
        }
    }
    let b = merge(&d);
    let (m, _) = members(&b);
    well_formed(&m);
    let var = text_of(&m, "Project/VariableSets/General.flvar");
    let toc = text_of(&m, "Project/TOCs/Master.fltoc");
    let glo = text_of(&m, "Project/Glossaries/Glossary.flglo");
    assert_eq!(count(&var, "<Variable "), 4, "{var}");
    assert_eq!(count(&toc, "Link=\"/"), 3, "{toc}");
    assert_eq!(count(&glo, "<Term>"), 1, "{glo}");
    for s in [&var, &toc, &glo] {
        assert!(!s.contains("<![CDATA[") && !s.contains('\u{1}') && !s.contains("&bogus;"), "{s}");
    }
    // the untouched members are the original bytes
    let (orig, _) = members(&src);
    for (n, data) in &orig {
        if n.ends_with(".css") || n.ends_with(".png") || n.ends_with(".flcts") || n.ends_with(".flprj") {
            assert_eq!(get(&m, n), data.as_slice(), "{n}");
        }
    }
}

#[test]
fn empty_translations_are_written_empty() {
    let src = project();
    let mut d = extract(&src);
    let ids: Vec<String> = ["Acme Writer", "Getting started", "spooler", "Hidden body text."]
        .iter()
        .map(|t| d.units[index_of(&d, t)].segments[0].id.clone())
        .collect();
    for id in &ids {
        d.set_target_empty(id).unwrap();
    }
    let (m, _) = members(&merge(&d));
    well_formed(&m);
    assert!(text_of(&m, "Project/VariableSets/General.flvar").contains("EvaluatedDefinition=\"\"></Variable>"));
    assert!(text_of(&m, "Project/TOCs/Master.fltoc").contains("Title=\"\""));
    assert!(text_of(&m, "Project/Glossaries/Glossary.flglo").contains("<Term></Term>"));
    assert!(!text_of(&m, "Content/Topic.htm").contains("Hidden body text."));
}

#[test]
fn merge_with_stamps_only_when_asked_or_translated() {
    let src = project();
    let mut d = extract(&src);
    d.target_lang = Some("ar".into());
    assert_eq!(Flare.merge(&d).unwrap(), src, "nothing translated: the original bytes");
    assert_eq!(Flare.merge_with(&d, &MergeOptions::new().with_output_encoding(OutputEncoding::Utf8)).unwrap(), src);
    let out = Flare.merge_with(&d, &MergeOptions::new().with_stamp_target_always(true)).unwrap();
    let (m, _) = members(&out);
    let t = text_of(&m, "Content/Topic.htm");
    assert!(t.contains("lang=\"ar\"") && t.contains("dir=\"rtl\""), "{t}");
    d.target_lang = Some("  ".into());
    assert_eq!(Flare.merge_with(&d, &MergeOptions::new().with_stamp_target_always(true)).unwrap(), src, "a blank language stamps nothing");
}

#[test]
fn conditional_text_around_blocks_matches_locos() {
    let t = |b: &str| topic(b, "<title>T</title>");
    for (body, want) in [
        ("<MadCap:conditionalText MadCap:conditions=\"x\"><p>A</p><p>B</p></MadCap:conditionalText><p>C</p>", vec!["T", "A", "B", "C"]),
        (
            "<div>Lead <MadCap:conditionalText MadCap:conditions=\"x\">print</MadCap:conditionalText><p>Para</p> tail</div>",
            vec!["T", "Lead \u{27E6}g1\u{27E7}print\u{27E6}/g1\u{27E7}", "Para", "tail"],
        ),
    ] {
        let src_topic = t(body);
        let src = proj(&src_topic);
        let mut d = extract(&src);
        assert_eq!(loc_texts(&d), want, "{body}");
        translate_all(&mut d);
        let b = merge(&d);
        let (m, _) = members(&b);
        well_formed(&m);
        let out = out_topic(&b);
        no_mask_markup(&out);
        assert_eq!(count(&out, "MadCap:conditionalText"), count(&src_topic, "MadCap:conditionalText"), "{out}");
    }
}
