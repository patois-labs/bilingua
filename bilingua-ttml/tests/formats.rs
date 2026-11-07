//! Every fixture in `tests/data`: round trips, the 11 languages and each encoding the filter reads.

mod common;

use bilingua_model::*;
use common::*;

include!("data_list.inc");

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

fn encoding(d: &Document) -> String {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => enc::base(encoding).to_string(),
        _ => String::new(),
    }
}

fn pseudo(d: &mut Document) {
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, data) in FILES {
        let d = extract(data);
        assert_eq!(F.merge(&d).unwrap(), data, "{name}");
        assert_identity(data);
        // a merge for a target language with nothing translated is also the original bytes (lead decision 11-04)
        let d = F.extract(data, &opts_for(Some("de-DE"))).unwrap();
        assert_eq!(F.merge(&d).unwrap(), data, "{name}: no stamp when nothing is translated");
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, data) in FILES {
        let before = extract(data);
        let mut d = before.clone();
        pseudo(&mut d);
        let out = F.merge(&d).unwrap();
        let after = extract(&out);
        assert_eq!(after.units.len(), before.units.len(), "{name}");
        assert_eq!(tag_count(&after), tag_count(&before), "{name}");
        assert_eq!(encoding(&after), encoding(&before), "{name}: encoding kept");
        for (i, (a, b)) in after.units.iter().zip(&before.units).enumerate() {
            assert_eq!(show(a), format!("[{}]", show(b)), "{name} unit {i}");
            assert_eq!(a.context.extra, b.context.extra, "{name}: timing never changes");
        }
        // and once more through the LocOS-style export (UTF-8 out)
        let utf8 = F.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        let again = extract(&utf8);
        assert_eq!(again.units.len(), before.units.len(), "{name} utf-8 export");
        assert!(encoding(&again).starts_with("utf-8"), "{name}");
    }
}

#[test]
fn eleven_languages_and_their_encodings() {
    let langs = ["en", "de", "fr", "es", "pt", "ru", "tr", "ar", "ja", "zh", "ko"];
    for l in langs {
        let data = FILES.iter().find(|(n, _)| *n == format!("lang_{l}")).unwrap().1;
        let u = texts(data);
        assert_eq!(u.len(), 5, "{l}");
        assert!(u[1].contains("{0}") && u[1].contains("{1}"), "{l}: {}", u[1]);
        assert!(u[2].contains("⟦x1⟧") && u[1].contains("⟦g1⟧"), "{l}: {u:?}");
        // target locale: the language is stamped, and an Arabic file is right to left
        let mut d = F.extract(data, &opts_for(Some(l))).unwrap();
        set(&mut d, 0, &format!("[{l}] {}", u[0]));
        let out = String::from_utf8(F.merge_with(&d, &MergeOptions::default()).unwrap()).unwrap();
        assert!(out.contains(&format!("xml:lang=\"{l}\"")));
        if l == "ar" {
            assert!(out.contains("<p tts:direction=\"rtl\" begin=\"1s\""), "{out}");
        } else {
            assert!(!out.contains("direction"));
        }
    }
    let legacy = [
        ("lang_ja_shiftjis", "shift_jis"),
        ("lang_ja_eucjp", "euc-jp"),
        ("lang_zh_gbk", "gbk"),
        ("lang_zh_big5", "big5"),
        ("lang_ko_euckr", "euc-kr"),
        ("lang_ar_cp1256", "windows-1256"),
        ("lang_ru_cp1251", "windows-1251"),
        ("lang_ru_koi8r", "koi8-r"),
        ("lang_de_latin1", "iso-8859-1"),
        ("lang_tr_cp1254", "windows-1254"),
        ("lang_ar_utf16le", "utf-16le"),
        ("lang_ja_utf16be", "utf-16be"),
        ("lang_en_utf8bom_crlf", "utf-8-bom"),
    ];
    for (name, want) in legacy {
        let data = FILES.iter().find(|(n, _)| *n == name).unwrap().1;
        let d = extract(data);
        let got = encoding(&d);
        assert!(got == want || got.replace('_', "-") == want || got == want.replace("iso-8859-1", "windows-1252"), "{name}: {got} vs {want}");
        assert_eq!(d.units.len(), 5, "{name}");
    }
}

#[test]
fn a_character_the_legacy_code_page_cannot_hold_becomes_a_character_reference() {
    let data = FILES.iter().find(|(n, _)| *n == "lang_de_latin1").unwrap().1;
    let mut d = F.extract(data, &opts_for(Some("de"))).unwrap();
    set(&mut d, 0, "Willkommen \u{65e5}\u{672c} \u{1f600}");
    let out = F.merge(&d).unwrap();
    assert!(out.starts_with(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>"));
    assert!(String::from_utf8_lossy(&out).contains("&#26085;&#26412; &#128512;"));
    assert_eq!(show(&extract(&out).units[0]), "Willkommen \u{65e5}\u{672c} \u{1f600}");
}

#[test]
fn edge_file_units() {
    let data = FILES.iter().find(|(n, _)| *n == "edge").unwrap().1;
    let got = units(data);
    assert_eq!(got.len(), 5, "{got:?}");
    // preserve keeps inner spacing; the p keeps its trailing space only as skeleton
    assert_eq!(got[0].1, "keep   spacing  ⟦g1⟧inner ⟦g2⟧nested⟦/g2⟧ end⟦/g1⟧");
    // layout whitespace around a <br/> vanishes; a foreign element is kept whole
    assert_eq!(got[1].1, "line one⟦x1⟧line two ⟦x2⟧ tail");
    assert_eq!(got[2].1, "⟦x1⟧⟦x2⟧© ☺ <tag> <raw & text>");
    assert_eq!(got[3].1, "⟦x1⟧after br pair");
    assert_eq!(got[4].1, "⟦g1⟧Colour⟦/g1⟧ at 5 > 3");
}

#[test]
fn the_2004_dfxp_draft_namespace_is_read_outside_the_locos_profile() {
    // the first DFXP working draft (2004-11) namespace, with its #styling namespace
    let data = br#"<tt xml:lang="en" xmlns="http://www.w3.org/2004/11/ttaf1">
  <head><styling xmlns:tts="http://www.w3.org/2004/11/ttaf1#styling"><style id="a" tts:color="white"/></styling></head>
  <body><div>
    <p begin="1.0" end="2.5" id="c1">The lighthouse keeper<br/>waves from the rocks.</p>
    <p begin="3.0" end="4.0" id="c2">Good night.</p>
  </div></body>
</tt>
"#;
    assert!(F.sniff(data));
    assert_eq!(texts(data).len(), 2);
    assert_identity(data);
    let mut d = extract(data);
    set(&mut d, 1, "Buenas noches.");
    let out = F.merge(&d).expect("merge");
    assert_eq!(texts(&out)[1], "Buenas noches.");
    // the locos profile keeps LocOS's refusal
    let locos = opts().with_profile("locos");
    assert!(F.extract(data, &locos).is_err());
}
