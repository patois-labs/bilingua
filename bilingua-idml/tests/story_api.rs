//! The public story-markup module on a bare story (an InCopy ICML file has this shape: no package around it).

use bilingua_idml::story::{self, Opts};
use bilingua_idml::xml::{ParseLimits, Tree};
use bilingua_model::{Inline, TagKind};

const ICML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Document DOMVersion=\"20.0\" Self=\"d\">\n<Story Self=\"u1\">\n<ParagraphStyleRange AppliedParagraphStyle=\"ParagraphStyle/Head\">\n<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content>Hello </Content></CharacterStyleRange>\n<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/Bold\"><Content>big</Content></CharacterStyleRange>\n<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/$ID/[No character style]\"><Content> world</Content><Br/></CharacterStyleRange>\n</ParagraphStyleRange>\n<ParagraphStyleRange><CharacterStyleRange><Content>Second</Content></CharacterStyleRange></ParagraphStyleRange>\n</Story>\n</Document>\n";

#[test]
fn bare_story_round_trip_through_the_public_api() {
    let mut tree = Tree::parse(ICML, &ParseLimits::default()).unwrap();
    assert_eq!(tree.to_xml(), ICML, "an unchanged tree writes back exactly");
    let paras = story::collect(&mut tree, &Opts::default());
    assert_eq!(paras.len(), 2);
    assert_eq!(story::style(&tree, &paras[0]), "Head");
    let tok = story::tokenize(&tree, &paras[0]);
    assert!(tok.has_text());
    let src = tok.core_inlines();
    assert_eq!(
        src,
        vec![
            Inline::text("Hello "),
            Inline::tag("1", TagKind::Open, "<CharacterStyleRange AppliedCharacterStyle=\"CharacterStyle/Bold\">"),
            Inline::text("big"),
            Inline::tag("1", TagKind::Close, "</CharacterStyleRange>"),
            Inline::text(" world"),
        ]
    );
    assert_eq!(tok.codes()["1"].label.as_deref(), Some("run"));
    let target = vec![Inline::tag("1", TagKind::Open, ""), Inline::text("gro\u{00df}"), Inline::tag("1", TagKind::Close, ""), Inline::text(" Welt, hallo")];
    let ranges = story::rebuild(&mut tree, &paras[0], &tok, &target);
    story::stamp_language(&mut tree, &ranges, "$ID/de_DE_2006");
    story::stamp_rtl(&mut tree, &ranges);
    let xml = tree.to_xml();
    assert!(xml.contains("<Br/>"), "{xml}");
    assert!(xml.contains("AppliedLanguage=\"$ID/de_DE_2006\""), "{xml}");
    assert!(xml.contains("ParagraphDirection=\"RightToLeftDirection\""), "{xml}");
    let again = Tree::parse(&xml, &ParseLimits::default()).unwrap();
    let mut again = again;
    let paras2 = story::collect(&mut again, &Opts::default());
    assert_eq!(paras2.len(), 2);
    let tok2 = story::tokenize(&again, &paras2[0]);
    assert_eq!(tok2.core_inlines().len(), 4);
}
