//! Round trips: an untouched merge is the original bytes (every encoding, BOM and newline style); a pseudo
//! translated merge reads back with the same units and tags; 11 languages; the legacy encodings LocOS reads.
mod common;
use bilingua_mdx::Mdx;
use bilingua_model::encoding_rs;
use bilingua_model::*;
use common::*;

fn tag_shape(u: &Unit) -> Vec<(String, TagKind)> {
    u.source().iter().filter_map(|i| if let Inline::Tag { id, kind, .. } = i { Some((id.clone(), *kind)) } else { None }).collect()
}

/// Every unit translated: its text wrapped, its tags kept in place.
fn pseudo(doc: &mut Document) {
    for i in 0..doc.units.len() {
        let id = doc.units[i].segments[0].id.clone();
        let mut t = vec![Inline::text("\u{ab}")];
        for x in doc.units[i].source() {
            t.push(match x {
                Inline::Text(s) => Inline::Text(format!("\u{a7}{s}")),
                tag => tag,
            });
        }
        t.push(Inline::text("\u{bb}"));
        doc.set_target(&id, t).unwrap();
    }
}

fn round_trip(data: &[u8], opts: &ExtractOptions) -> Vec<u8> {
    let doc = Mdx.extract(data, opts).unwrap();
    assert_eq!(Mdx.merge(&doc).unwrap(), data, "untouched merge");
    let mut doc2 = Mdx.extract(data, opts).unwrap();
    pseudo(&mut doc2);
    let out = Mdx.merge(&doc2).unwrap();
    let back = Mdx.extract(&out, opts).unwrap();
    assert_eq!(back.units.len(), doc.units.len(), "unit count after a translated merge");
    for (a, b) in doc.units.iter().zip(&back.units) {
        assert_eq!(tag_shape(a), tag_shape(b), "tags of {}", a.id);
    }
    out
}

fn sample(title: &str, body: &str) -> String {
    format!(
        "---\ntitle: {title}\ndescription: \"{body}\"\n---\n\nimport {{ Kbd }} from './kbd.js'\n\n# {title}\n\nPress <Kbd>Ctrl</Kbd> and {{count}} to {body}.\n\n- {body} `code {{x}}`\n- **{title}** <Icon name=\"x\" />\n\n> {body}\n\n<Note title=\"{title}\" type=\"info\">\n{body}\n</Note>\n\n| a | b |\n|---|---|\n| {title} | {body} |\n\n```js\nconst x = <Hidden>no</Hidden>\n```\n\n[ref]: http://x.y \"{title}\"\n\nThe <Abbr title=\"{body}\">API</Abbr> is stable.\n"
    )
}

const LANGS: &[(&str, &str, &str)] = &[
    ("en", "Getting started", "Save your work before you leave"),
    ("de", "Erste Schritte \u{fc}ber Gr\u{f6}\u{df}e", "Speichern Sie Ihre Arbeit, bevor Sie gehen"),
    ("fr", "Pour commencer \u{e0} l'\u{e9}t\u{e9}", "Enregistrez votre travail avant de partir"),
    ("es", "Primeros pasos con ni\u{f1}os", "Guarde su trabajo antes de salir"),
    ("ru", "\u{41d}\u{430}\u{447}\u{430}\u{43b}\u{43e} \u{440}\u{430}\u{431}\u{43e}\u{442}\u{44b}", "\u{421}\u{43e}\u{445}\u{440}\u{430}\u{43d}\u{438}\u{442}\u{435} \u{440}\u{430}\u{431}\u{43e}\u{442}\u{443} \u{43f}\u{435}\u{440}\u{435}\u{434} \u{443}\u{445}\u{43e}\u{434}\u{43e}\u{43c}"),
    ("ar", "\u{627}\u{644}\u{628}\u{62f}\u{621} \u{633}\u{631}\u{64a}\u{639}\u{627}", "\u{627}\u{62d}\u{641}\u{638} \u{639}\u{645}\u{644}\u{643} \u{642}\u{628}\u{644} \u{627}\u{644}\u{645}\u{63a}\u{627}\u{62f}\u{631}\u{629}"),
    ("he", "\u{5d4}\u{5ea}\u{5d7}\u{5dc}\u{5d4}", "\u{5e9}\u{5de}\u{5e8}\u{5d5} \u{5d0}\u{5ea} \u{5d4}\u{5e2}\u{5d1}\u{5d5}\u{5d3}\u{5d4} \u{5dc}\u{5e4}\u{5e0}\u{5d9} \u{5d4}\u{5d9}\u{5e6}\u{5d9}\u{5d0}\u{5d4}"),
    ("ja", "\u{306f}\u{3058}\u{3081}\u{306b}", "\u{4f5c}\u{696d}\u{3092}\u{4fdd}\u{5b58}\u{3057}\u{3066}\u{304b}\u{3089}\u{7d42}\u{4e86}\u{3057}\u{3066}\u{304f}\u{3060}\u{3055}\u{3044}\u{3002}"),
    ("zh", "\u{5165}\u{95e8}\u{6307}\u{5357}", "\u{79bb}\u{5f00}\u{524d}\u{8bf7}\u{4fdd}\u{5b58}\u{60a8}\u{7684}\u{5de5}\u{4f5c}\u{3002}"),
    ("ko", "\u{c2dc}\u{c791}\u{d558}\u{ae30}", "\u{b5a0}\u{b098}\u{ae30} \u{c804}\u{c5d0} \u{c791}\u{c5c5}\u{c744} \u{c800}\u{c7a5}\u{d558}\u{c138}\u{c694}."),
    ("hi", "\u{936}\u{941}\u{930}\u{941}\u{906}\u{924}", "\u{91c}\u{93e}\u{928}\u{947} \u{938}\u{947} \u{92a}\u{939}\u{932}\u{947} \u{905}\u{92a}\u{928}\u{93e} \u{915}\u{93e}\u{92e} \u{938}\u{939}\u{947}\u{91c}\u{947}\u{902}"),
];

#[test]
fn eleven_languages() {
    assert!(LANGS.len() >= 11);
    for (lang, title, body) in LANGS {
        let src = sample(title, body);
        let out = round_trip(src.as_bytes(), &opts());
        let out = String::from_utf8(out).unwrap();
        // the structure that must not move
        assert!(out.contains("import { Kbd } from './kbd.js'"), "{lang}");
        assert!(out.contains("const x = <Hidden>no</Hidden>"), "{lang}");
        assert!(out.contains("<Kbd>\u{a7}Ctrl</Kbd>") && out.contains("{count}"), "{lang}");
        assert!(out.contains(&format!("# \u{ab}\u{a7}{title}\u{bb}")), "{lang}: {out}");
        assert!(out.contains(&format!("<Note title=\"\u{ab}\u{a7}{title}\u{bb}\" type=\"info\">")), "{lang}");
        assert!(out.contains("[ref]: http://x.y \"\u{ab}\u{a7}"), "{lang}");
        assert!(out.contains("title: \u{ab}\u{a7}") || out.contains("title: \"\u{ab}\u{a7}") || out.contains("title: '\u{ab}\u{a7}"), "{lang}");
    }
}

#[test]
fn newline_styles_and_bom_survive() {
    let base = sample("Titel", "Speichern Sie Ihre Arbeit");
    for nl in ["\n", "\r\n", "\r"] {
        let src = base.replace('\n', nl);
        let out = round_trip(src.as_bytes(), &opts());
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains(&format!("# \u{ab}\u{a7}Titel\u{bb}{nl}")), "{nl:?}");
        let stripped = out.replace("\r\n", "").replace(['\r', '\n'], "");
        assert!(!stripped.is_empty());
        if nl == "\r\n" {
            assert!(!out.replace("\r\n", "").contains('\n'), "a bare LF crept into a CRLF file");
        }
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(src.as_bytes());
        let out = round_trip(&bom, &opts());
        assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    }
    // no final newline
    round_trip(base.trim_end().as_bytes(), &opts());
}

fn encode_as(e: &'static encoding_rs::Encoding, s: &str) -> Vec<u8> {
    let (b, _, bad) = e.encode(s);
    assert!(!bad, "{} cannot hold the sample", e.name());
    b.into_owned()
}

#[test]
fn utf16_is_kept() {
    let src = sample("Titel", "Speichern Sie Ihre Arbeit");
    for (e, bom) in [(encoding_rs::UTF_16LE, [0xFF, 0xFE]), (encoding_rs::UTF_16BE, [0xFE, 0xFF])] {
        let mut b = bom.to_vec();
        for u in src.encode_utf16() {
            b.extend_from_slice(&if e == encoding_rs::UTF_16LE { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        let out = round_trip(&b, &opts());
        assert!(out.starts_with(&bom), "{}", e.name());
    }
}

#[test]
fn legacy_encodings_are_kept() {
    // (source language, encoding, title, body) - the code page LocOS picks from the project's source language
    let cases: &[(&str, &'static encoding_rs::Encoding, &str, &str)] = &[
        ("fr-FR", encoding_rs::WINDOWS_1252, "Pour commencer \u{e0} l'\u{e9}t\u{e9}", "Enregistrez votre travail"),
        ("ru-RU", encoding_rs::WINDOWS_1251, "\u{41d}\u{430}\u{447}\u{430}\u{43b}\u{43e}", "\u{421}\u{43e}\u{445}\u{440}\u{430}\u{43d}\u{438}\u{442}\u{435} \u{440}\u{430}\u{431}\u{43e}\u{442}\u{443}"),
        ("el-GR", encoding_rs::WINDOWS_1253, "\u{391}\u{3c1}\u{3c7}\u{3ae}", "\u{391}\u{3c0}\u{3bf}\u{3b8}\u{3b7}\u{3ba}\u{3b5}\u{3cd}\u{3c3}\u{3c4}\u{3b5}"),
        ("ar-SA", encoding_rs::WINDOWS_1256, "\u{627}\u{644}\u{628}\u{62f}\u{621}", "\u{627}\u{62d}\u{641}\u{638} \u{639}\u{645}\u{644}\u{643}"),
        ("he-IL", encoding_rs::WINDOWS_1255, "\u{5d4}\u{5ea}\u{5d7}\u{5dc}\u{5d4}", "\u{5e9}\u{5de}\u{5e8}\u{5d5} \u{5d0}\u{5ea} \u{5d4}\u{5e2}\u{5d1}\u{5d5}\u{5d3}\u{5d4}"),
        ("ja-JP", encoding_rs::SHIFT_JIS, "\u{306f}\u{3058}\u{3081}\u{306b}", "\u{4f5c}\u{696d}\u{3092}\u{4fdd}\u{5b58}\u{3057}\u{3066}\u{304f}\u{3060}\u{3055}\u{3044}"),
        ("zh-CN", encoding_rs::GBK, "\u{5165}\u{95e8}\u{6307}\u{5357}", "\u{8bf7}\u{4fdd}\u{5b58}\u{60a8}\u{7684}\u{5de5}\u{4f5c}"),
        ("ko-KR", encoding_rs::EUC_KR, "\u{c2dc}\u{c791}\u{d558}\u{ae30}", "\u{c791}\u{c5c5}\u{c744} \u{c800}\u{c7a5}\u{d558}\u{c138}\u{c694}"),
    ];
    for (lang, e, title, body) in cases {
        let bytes = encode_as(e, &sample(title, body));
        let o = ExtractOptions::new().with_source_lang(lang.to_string());
        let out = round_trip(&bytes, &o);
        // the translation only has characters of the code page (the pseudo marks are Latin-1 but not in every
        // page), so the file is read back in the same way and still has its units
        let back = Mdx.extract(&out, &o).unwrap();
        assert!(!back.units.is_empty(), "{lang}");
    }
    // a translation that does not fit the code page is written as UTF-8
    let bytes = encode_as(encoding_rs::WINDOWS_1252, "Caf\u{e9} <B>cr\u{e8}me</B>.\n");
    let o = ExtractOptions::new().with_source_lang("fr-FR");
    let mut doc = Mdx.extract(&bytes, &o).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("\u{65e5}\u{672c}\u{8a9e}")]).unwrap();
    assert_eq!(Mdx.merge(&doc).unwrap(), "\u{65e5}\u{672c}\u{8a9e}\n".as_bytes());
    // a translation that fits keeps the code page
    let mut doc = Mdx.extract(&bytes, &o).unwrap();
    doc.set_target(&id, vec![Inline::text("Caf\u{e9} noir")]).unwrap();
    assert_eq!(Mdx.merge(&doc).unwrap(), b"Caf\xe9 noir\n");
}

#[test]
fn guide_round_trips() {
    let out = round_trip(&guide(), &opts());
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("title: \u{ab}\u{a7}Getting started\u{bb}") || out.contains("title: \"\u{ab}\u{a7}Getting started\u{bb}\""), "{out}");
    assert!(out.contains("{props.count}") && out.contains("<Kbd>\u{a7}Ctrl</Kbd>"));
}

#[test]
fn paragraph_granularity_is_the_only_mode() {
    let o = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    let doc = Mdx.extract(&guide(), &o).unwrap();
    assert!(doc.units.iter().all(|u| u.segments.len() == 1));
    let ids: std::collections::HashSet<_> = doc.units.iter().map(|u| u.id.clone()).collect();
    assert_eq!(ids.len(), doc.units.len());
    // stable ids: the same file gives the same ids
    let again = Mdx.extract(&guide(), &o).unwrap();
    assert_eq!(doc.units.iter().map(|u| &u.id).collect::<Vec<_>>(), again.units.iter().map(|u| &u.id).collect::<Vec<_>>());
}

#[test]
fn json_skeleton_round_trip() {
    // a document survives a trip through JSON (the editor and the Python binding do this)
    let doc = Mdx.extract(&guide(), &opts()).unwrap();
    let j = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&j).unwrap();
    assert_eq!(Mdx.merge(&back).unwrap(), guide());
    let mut d2 = back;
    pseudo(&mut d2);
    let j = serde_json::to_string(&d2).unwrap();
    let d3: Document = serde_json::from_str(&j).unwrap();
    assert_eq!(Mdx.merge(&d3).unwrap(), Mdx.merge(&d2).unwrap());
}
