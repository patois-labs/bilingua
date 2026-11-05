//! Behaviour beyond the LocOS suite: existing translations, edited targets, comments, CDATA, namespaces, newlines,
//! and the segments of a unit that was split.

mod common;

use bilingua_model::*;
use common::*;

fn wrap(inner: &str) -> Vec<u8> {
    format!(
        "<?xml version='1.0'?>\r\n<TRADOStag Version=\"2.0\"><FrontMatter><UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"/></FrontMatter><Body><Raw>\r\n{inner}\r\n</Raw></Body></TRADOStag>"
    )
    .into_bytes()
}

fn s(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap()
}

#[test]
fn an_edited_existing_translation_is_written() {
    let raw = wrap("<Tu MatchPercent=\"100\" Origin=\"manual\"><Tuv Lang=\"EN-US\">Hello</Tuv><Tuv Lang=\"DE-DE\">Hallo</Tuv></Tu>");
    let mut d = extract_with(&DEFAULT, &raw);
    assert_eq!(tok(&d.units[0].segments[0].target), "Hallo");
    // setting the same text is not an edit
    set(&mut d, 0, "Hallo");
    assert_eq!(DEFAULT.merge(&d).unwrap(), raw);
    // a person's edit keeps the file's own origin and match
    set(&mut d, 0, "Guten Tag");
    d.units[0].segments[0].origin = Some(Origin::new(OriginKind::Human).with_match(100));
    let out = s(DEFAULT.merge(&d).unwrap());
    assert!(out.contains("<Tu MatchPercent=\"100\" Origin=\"manual\"><Tuv Lang=\"EN-US\">Hello</Tuv><Tuv Lang=\"DE-DE\">Guten Tag</Tuv></Tu>"), "{out}");
    // machine output is written as mt with match 0
    let mut d = extract_with(&DEFAULT, &raw);
    d.units[0].segments[0].origin = None;
    set(&mut d, 0, "Moin");
    assert!(s(DEFAULT.merge(&d).unwrap()).contains("<Tu MatchPercent=\"0\" Origin=\"mt\">"));
}

#[test]
fn states_and_origins_are_read_from_the_tu() {
    let raw = wrap(
        "<Tu Origin=\"xtranslate\" MatchPercent=\"101\"><Tuv Lang=\"EN-US\">A</Tuv><Tuv Lang=\"DE-DE\">B</Tuv></Tu>\
         <Tu Origin=\"TM1\" MatchPercent=\"90\"><Tuv Lang=\"EN-US\">C</Tuv><Tuv Lang=\"DE-DE\">D</Tuv></Tu>\
         <Tu Origin=\"mt\" MatchPercent=\"0\"><Tuv Lang=\"EN-US\">E</Tuv><Tuv Lang=\"DE-DE\">F</Tuv></Tu>",
    );
    let d = extract_with(&DEFAULT, &raw);
    let seg = |i: usize| &d.units[i].segments[0];
    assert_eq!((seg(0).state, seg(0).origin.clone()), (SegState::Translated, Some(Origin::new(OriginKind::Tm).with_match(101))));
    assert_eq!(seg(1).state, SegState::Draft);
    assert_eq!(seg(1).origin, Some(Origin { kind: OriginKind::Other, name: Some("TM1".into()), match_pct: Some(90) }));
    assert_eq!((seg(2).state, seg(2).origin.clone()), (SegState::Draft, Some(Origin::new(OriginKind::Mt).with_match(0))));
}

#[test]
fn target_tags_take_the_ids_of_the_source_tags() {
    let raw = wrap(
        "<Tu><Tuv Lang=\"EN-US\">A <ut Type=\"start\">&lt;b&gt;</ut>b<ut Type=\"end\">&lt;/b&gt;</ut> <ut>&lt;br&gt;</ut></Tuv>\
         <Tuv Lang=\"DE-DE\"><ut>&lt;br&gt;</ut> <ut Type=\"start\">&lt;b&gt;</ut>B<ut Type=\"end\">&lt;/b&gt;</ut> <ut>&lt;hr&gt;</ut></Tuv></Tu>",
    );
    let d = extract_with(&DEFAULT, &raw);
    // the target's br is the source's code 3, its b pair is code 1, and the hr only the target has gets t1
    assert_eq!(tok(&d.units[0].segments[0].source), "A ⟦g1⟧b⟦/g1⟧ ⟦x2⟧");
    let t: Vec<String> = d.units[0].segments[0]
        .target
        .iter()
        .filter_map(|i| if let Inline::Tag { id, kind, .. } = i { Some(format!("{id}{kind:?}")) } else { None })
        .collect();
    assert_eq!(t, ["2Standalone", "1Open", "1Close", "t1Standalone"]);
    assert_eq!(DEFAULT.merge(&d).unwrap(), raw);
}

#[test]
fn comments_cdata_and_namespaces() {
    let raw = s(wrap("")).replace("<Raw>\r\n\r\n</Raw>", "<Raw><!-- note --><Tu><Tuv Lang=\"EN-US\">A<!-- x -->B <![CDATA[C & <D>]]></Tuv></Tu></Raw>").into_bytes();
    let d = extract_with(&DEFAULT, &raw);
    assert_eq!(tok(&d.units[0].segments[0].source), "AB C & <D>");
    assert_eq!(DEFAULT.merge(&d).unwrap(), raw);
    let ns = "<t:TRADOStag xmlns:t=\"urn:x\"><t:FrontMatter><t:UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"/></t:FrontMatter><t:Body><t:Raw><t:Tu><t:Tuv Lang=\"EN-US\">Hi <t:ut Type=\"start\">&lt;b&gt;</t:ut>x<t:ut Type=\"end\">&lt;/b&gt;</t:ut></t:Tuv></t:Tu></t:Raw></t:Body></t:TRADOStag>";
    let out = s(translate_with(&DEFAULT, ns.as_bytes(), &[(0, "Hallo ⟦g1⟧y⟦/g1⟧")]));
    assert!(out.contains("<t:Tuv Lang=\"DE-DE\">Hallo <t:ut Type=\"start\">&lt;b&gt;</t:ut>y<t:ut Type=\"end\">&lt;/b&gt;</t:ut></t:Tuv>"), "{out}");
    // an undeclared prefix is not well-formed
    assert!(DEFAULT.extract(b"<t:TRADOStag/>", &opts()).is_err());
}

#[test]
fn newlines_follow_the_file() {
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">One</Tuv></Tu>");
    let out = s(translate_with(&DEFAULT, &raw, &[(0, "Eins\nZwei\r\nDrei")]));
    assert!(out.contains("Eins\r\nZwei\r\nDrei"), "{out:?}");
    let lf = s(raw.clone()).replace("\r\n", "\n").into_bytes();
    let out = s(translate_with(&DEFAULT, &lf, &[(0, "Eins\nZwei")]));
    assert!(out.contains("Eins\nZwei") && !out.contains('\r'));
    // a source with a line break reads it as one
    let two = wrap("<Tu><Tuv Lang=\"EN-US\">a\r\nb</Tuv></Tu>");
    assert_eq!(units_with(&DEFAULT, &two), [(0, "a\nb".to_string())]);
}

#[test]
fn characters_are_escaped_and_xml_safe() {
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">One</Tuv></Tu>");
    let out = s(translate_with(&DEFAULT, &raw, &[(0, "A & B < C > D ]]> E \u{1} \u{b}F")]));
    assert!(out.contains("A &amp; B &lt; C > D ]]&gt; E  \r\nF"), "{out:?}");
    DEFAULT.extract(out.as_bytes(), &opts()).expect("well-formed");
}

#[test]
fn a_split_unit_writes_its_segments_joined() {
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">One. Two.</Tuv></Tu>");
    let mut d = extract_with(&DEFAULT, &raw);
    let u = &mut d.units[0];
    u.segments = vec![
        Segment::new("u1#1", vec![Inline::text("One.")]).with_target(vec![Inline::text("Eins.")]).with_state(SegState::Draft),
        Segment::new("u1#2", vec![Inline::text("Two.")]),
    ];
    u.separators = vec![" ".into()];
    let out = s(DEFAULT.merge(&d).unwrap());
    assert!(out.contains("<Tuv Lang=\"DE-DE\">Eins. Two.</Tuv>"), "{out}");
    // untouched split units still give the original bytes
    d.units[0].segments[0].target.clear();
    assert_eq!(DEFAULT.merge(&d).unwrap(), raw);
}

#[test]
fn nested_tu_is_inline_and_header_lookups() {
    // a Tu inside a Tuv is a code of the outer segment, not a segment
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">A <Tu><Tuv Lang=\"EN-US\">inner</Tuv></Tu> B</Tuv></Tu>");
    let d = extract_with(&DEFAULT, &raw);
    assert_eq!(d.units.len(), 1);
    assert_eq!(tok(&d.units[0].segments[0].source), "A ⟦x1⟧ B");
    // a Tuv in a third language is not the target when the target language exists
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">A</Tuv><Tuv Lang=\"FR-FR\">Fr</Tuv><Tuv Lang=\"DE-DE\">De</Tuv></Tu>");
    let d = extract_with(&DEFAULT, &raw);
    assert_eq!(tok(&d.units[0].segments[0].target), "De");
    // the source language picks the source Tuv
    let raw = wrap("<Tu><Tuv Lang=\"DE-DE\">De</Tuv><Tuv Lang=\"EN-US\">A</Tuv></Tu>");
    assert_eq!(units_with(&DEFAULT, &raw), [(0, "A".to_string())]);
    // a stamped language on a file whose UserSettings is missing is not invented
    let nolang = b"<TRADOStag><Body><Raw><Tu><Tuv Lang=\"EN-US\">A</Tuv></Tu></Raw></Body></TRADOStag>";
    let mut d = DEFAULT.extract(nolang, &opts().with_target_lang("fr-FR")).unwrap();
    set(&mut d, 0, "B");
    let out = s(DEFAULT.merge(&d).unwrap());
    assert!(out.contains("<Tuv Lang=\"FR-FR\">B</Tuv>") && !out.contains("TargetLanguage"), "{out}");
}
