//! .xcloc behaviour beyond the LocOS script: languages, tag spelling, XLIFF 2, untouched members.
#[path = "xcloc_common/mod.rs"]
mod xcloc_common;
use bilingua_model::*;
use xcloc_common::*;

const LANGS: &[(&str, &str)] = &[
    ("de", "de"),
    ("fr-CA", "fr-CA"),
    ("es_419", "es-419"),
    ("ru", "ru"),
    ("pt_br", "pt-BR"),
    ("ar", "ar"),
    ("ja", "ja"),
    ("zh_hans_cn", "zh-Hans-CN"),
    ("ko", "ko"),
    ("he", "he"),
    ("tr", "tr"),
];

#[test]
fn eleven_languages_stamp_the_language_and_keep_everything_else() {
    let src = Bundle::default().build();
    for (given, tag) in LANGS {
        let mut d = ex_with(&src, Some(given));
        let n = d.units.len();
        for pos in 0..n {
            let t = text(&d.units[pos]);
            put(&mut d, pos, &format!("\u{ab}{given}\u{bb} {t}"));
        }
        let out = merge(&d);
        let cj = String::from_utf8(member(&out, "fr.xcloc/contents.json")).unwrap();
        assert!(cj.contains(&format!("\"targetLocale\" : \"{tag}\"")), "{given}: {cj}");
        let x = String::from_utf8(member(&out, "fr.xcloc/Localized Contents/fr.xliff")).unwrap();
        assert!(x.contains(&format!("target-language=\"{tag}\"")), "{given}: {x}");
        assert!(x.contains(&format!("\u{ab}{given}\u{bb} Hello %@")), "{given}");
        // the other members are the same bytes
        for m in ["fr.xcloc/Localized Contents/fr.lproj/logo.png", "fr.xcloc/Notes/screen.txt", "fr.xcloc/Source Contents/MyApp/Localizable.xcstrings", "__MACOSX/fr.xcloc/._contents.json"] {
            assert_eq!(member(&out, m), member(&src, m), "{m}");
        }
        // reading the result gives the same sources
        let back = ex(&out);
        assert_eq!(back.units.len(), n);
        assert_eq!(back.units.iter().map(text).collect::<Vec<_>>().len(), n);
    }
}

#[test]
fn the_language_alone_changes_two_members() {
    let src = Bundle::default().build();
    let out = merge(&ex_with(&src, Some("de")));
    let (a, b) = (members(&src), members(&out));
    let changed: Vec<&String> = a.iter().zip(&b).filter(|(x, y)| x.3 != y.3).map(|(x, _)| &x.0).collect();
    assert_eq!(changed, ["fr.xcloc/contents.json", "fr.xcloc/Localized Contents/fr.xliff"]);
    assert_eq!(comment(&out), b"made by hand");
}

#[test]
fn xliff_two_gets_trg_lang() {
    let x2 = br#"<?xml version="1.0"?>
<xliff xmlns="urn:oasis:names:tc:xliff:document:2.0" version="2.0" srcLang="en" trgLang="fr">
  <file id="f1">
    <unit id="u1">
      <segment>
        <source>Hello</source>
      </segment>
    </unit>
  </file>
</xliff>
"#;
    let src = Bundle { xliffs: vec![("fr.xliff", x2)], ..Default::default() }.build();
    let mut d = ex_with(&src, Some("de"));
    put(&mut d, 0, "Hallo");
    let out = merge(&d);
    let x = String::from_utf8(member(&out, "fr.xcloc/Localized Contents/fr.xliff")).unwrap();
    assert!(x.contains("trgLang=\"de\"") && x.contains("Hallo"), "{x}");
    assert!(!x.contains("target-language"));
}

#[test]
fn a_file_without_a_target_language_gets_one() {
    let x = String::from_utf8(XLIFF.to_vec()).unwrap().replace(" target-language=\"fr\"", "");
    let src = Bundle { xliffs: vec![("fr.xliff", x.as_bytes())], ..Default::default() }.build();
    let out = merge(&ex_with(&src, Some("de")));
    let got = String::from_utf8(member(&out, "fr.xcloc/Localized Contents/fr.xliff")).unwrap();
    assert!(got.contains("datatype=\"plaintext\" target-language=\"de\""), "{got}");
}

#[test]
fn a_comment_and_cdata_do_not_fool_the_language_stamp() {
    let x = String::from_utf8(XLIFF.to_vec()).unwrap().replace("<body>", "<!-- <file target-language=\"zz\"> --><body>");
    let src = Bundle { xliffs: vec![("fr.xliff", x.as_bytes())], ..Default::default() }.build();
    let out = merge(&ex_with(&src, Some("de")));
    let got = String::from_utf8(member(&out, "fr.xcloc/Localized Contents/fr.xliff")).unwrap();
    assert!(got.contains("<!-- <file target-language=\"zz\"> -->") && got.contains("target-language=\"de\""), "{got}");
}

#[test]
fn an_unusable_language_is_ignored() {
    let src = Bundle::default().build();
    for bad in ["", "  ", "<script>", "x y", "123"] {
        let d = ex_with(&src, Some(bad));
        assert_eq!(merge(&d), src, "{bad:?}");
    }
}

#[test]
fn a_translated_state_new_target_becomes_translated_and_others_keep_their_state() {
    let x = String::from_utf8(XLIFF.to_vec()).unwrap().replace("<target state=\"new\">%1$lld items in %2$@</target>", "<target state=\"final\">%1$lld items in %2$@</target>");
    let src = Bundle { xliffs: vec![("fr.xliff", x.as_bytes())], ..Default::default() }.build();
    let mut d = ex(&src);
    put(&mut d, 0, "Bonjour %@");
    put(&mut d, 1, "%1$lld objets dans %2$@");
    let out = merge(&d);
    let got = String::from_utf8(member(&out, "fr.xcloc/Localized Contents/fr.xliff")).unwrap();
    assert!(got.contains("<target state=\"translated\">Bonjour %@</target>"), "{got}");
    // a state other than new/needs-translation is a reviewer's decision; it moves to translated like LocOS does
    assert!(got.contains(">%1$lld objets dans %2$@</target>"), "{got}");
}

#[test]
fn only_the_touched_xliff_is_rewritten() {
    let two = Bundle { xliffs: vec![("fr.xliff", XLIFF), ("fr-extra.xliff", XLIFF2)], ..Default::default() }.build();
    let mut d = ex(&two);
    put(&mut d, 3, "Mon appli");
    let out = merge(&d);
    let (a, b) = (members(&two), members(&out));
    let changed: Vec<&String> = a.iter().zip(&b).filter(|(x, y)| x.3 != y.3).map(|(x, _)| &x.0).collect();
    assert_eq!(changed, ["fr.xcloc/Localized Contents/fr-extra.xliff"]);
}

#[test]
fn macos_shadow_files_are_ignored_even_when_they_look_like_a_catalog() {
    let shadow = vec![
        ("__MACOSX/other.xcloc/contents.json", CONTENTS.to_vec()),
        ("__MACOSX/other.xcloc/Localized Contents/fr.xliff", XLIFF.to_vec()),
    ];
    let src = Bundle { extra: shadow, ..Default::default() }.build();
    assert_eq!(ex(&src).units.len(), 3);
}

#[test]
fn the_skeleton_of_another_crate_is_refused() {
    let src = Bundle::default().build();
    let mut d = ex(&src);
    if let Skeleton::Zip { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-docx", "0.1.0", 1));
    }
    assert!(matches!(bilingua_xcstrings::Xcloc.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn doc_languages_come_from_the_options_and_the_bundle() {
    let src = Bundle::default().build();
    let d = ex(&src);
    assert_eq!((d.source_lang.as_deref(), d.target_lang.as_deref()), (Some("en"), None));
    assert!(d.units.iter().all(|u| u.translate));
}
