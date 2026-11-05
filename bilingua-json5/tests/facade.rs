//! The facade surface (the equivalent of the LocOS `surfaces` and `end_to_end_jsonc` cases): the format is
//! registered, `.json5` and `.jsonc` are detected by extension, `.json` stays with the JSON filter, and a `.jsonc`
//! file goes through extract, translate and merge in every granularity.

use bilingua::{Granularity, Inline};

const SETTINGS: &[u8] = include_bytes!("data/settings.jsonc");

fn opts(g: Granularity) -> bilingua::ExtractOptions {
    bilingua::ExtractOptions { source_lang: Some("en".into()), target_lang: Some("de-DE".into()), granularity: g, include_locked: false, ..Default::default() }
}

#[test]
fn registered_and_detected_by_extension() {
    let all = bilingua::filters();
    let f = all.iter().find(|f| f.id() == "json5").expect("json5 registered");
    assert_eq!(f.extensions(), &["json5", "jsonc"]);
    assert_eq!(bilingua::detect("a.json5", b"{}").unwrap().id(), "json5");
    assert_eq!(bilingua::detect("settings.JSONC", b"{}").unwrap().id(), "json5");
    assert_eq!(bilingua::detect("a.json", b"{}").unwrap().id(), "json");
    // extension-less JSON stays with the JSON filter (JSON5 is claimed by extension only)
    assert_eq!(bilingua::detect("noext", b"{\"a\": \"b\"}").unwrap().id(), "json");
}

#[test]
fn jsonc_end_to_end_in_every_granularity() {
    for g in [Granularity::Sentence, Granularity::Paragraph, Granularity::Document] {
        let mut doc = bilingua::extract("settings.jsonc", SETTINGS, &opts(g)).unwrap();
        assert_eq!(doc.format, "json5");
        assert_eq!(bilingua::merge(&doc).unwrap(), SETTINGS, "{g:?} untouched");
        let ids: Vec<String> = doc.segments().map(|(_, s)| s.id.clone()).collect();
        for id in &ids {
            let src = doc.find(id).unwrap().1.source.clone();
            let mut t = vec![Inline::text("[de-DE] ")];
            t.extend(src);
            doc.set_target(id, t).unwrap();
        }
        let out = bilingua::merge(&doc).unwrap();
        let back = bilingua::extract("settings.de.jsonc", &out, &opts(Granularity::Paragraph)).unwrap();
        let texts: Vec<String> = back.units.iter().map(|u| bilingua::plain_text(&u.source())).collect();
        assert_eq!(texts[0], "[de-DE] My Workspace");
        assert_eq!(texts.len(), 4);
        let o = String::from_utf8(out).unwrap();
        assert!(o.contains("// Editor settings (VS Code style JSON with Comments)"));
        assert!(o.contains("\"tabSize\": 4,"));
    }
}

#[test]
fn json5_in_the_facade_uses_the_skeleton_stamp() {
    let doc = bilingua::extract("a.json5", b"{a: 'Hi'}", &opts(Granularity::Paragraph)).unwrap();
    let p = doc.skeleton.parser().unwrap();
    assert_eq!(p.name, "bilingua-json5");
    assert_eq!(p.schema, bilingua_json5::SKELETON_SCHEMA);
}
