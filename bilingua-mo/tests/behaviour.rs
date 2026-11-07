//! Behaviour beyond the LocOS suite: units and codes, round trips on every fixture, eleven languages, the legacy
//! code pages, header handling, duplicate keys and tampered documents.

mod common;
use bilingua_model::*;
use bilingua_mo::Mo;
use common::*;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{DATA}/{name}")).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn all_fixtures() -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(DATA)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            n.ends_with(".mo").then(|| (n.clone(), fixture(&n)))
        })
        .collect();
    v.sort();
    v
}

fn tags(u: &Unit) -> Vec<(String, bool)> {
    u.segments[0]
        .source
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { display, free, .. } => Some((display.clone(), *free)),
            _ => None,
        })
        .collect()
}

#[test]
fn fixtures_match_the_helper_packer() {
    // The files in tests/data are what the independent packer makes: the helper and the files cannot drift.
    assert_eq!(fixture("catalog_le.mo"), mo_le());
    assert_eq!(fixture("catalog_be_hash.mo"), mo_be_hash());
    assert_eq!(fixture("sparse.mo"), pack(&pairs(&[("", HEADER_FR), ("Keep", "Garder"), ("Skip", "")])));
}

#[test]
fn units_codes_and_ids() {
    let d = ex(&mo_le());
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["e2.0", "e2.1", "e3", "e4", "e5"]);
    // plural pair: forms, style, key
    assert_eq!(d.units[0].context.style.as_deref(), Some("msgstr[0]"));
    assert_eq!(d.units[1].context.style.as_deref(), Some("msgstr[1]"));
    assert_eq!(d.units[1].context.extra.get("plural").map(String::as_str), Some("1"));
    // context
    assert_eq!(d.units[3].context.key.as_deref(), Some("menu :: Open"));
    assert_eq!(d.units[3].context.extra.get("msgctxt").map(String::as_str), Some("menu"));
    assert_eq!(d.units[4].context.key.as_deref(), Some("state :: Open"));
    // existing translations are the targets
    assert_eq!(flat_target(&d.units[0]), "%d fichier");
    assert_eq!(flat_target(&d.units[1]), "%d fichiers");
    assert_eq!(d.units[3].segments[0].state, SegState::Translated);
    // placeholders: %d keeps its order (not free), %(name)s and {0} may move (free); every code is a flat placeholder
    assert_eq!(tags(&d.units[0]), [("%d".to_string(), false)]);
    assert_eq!(tags(&d.units[2]), [("%(name)s".to_string(), true), ("{0}".to_string(), true)]);
    for u in &d.units {
        for c in u.codes.values() {
            assert_eq!(c.label.as_deref(), Some("placeholder"));
            assert!(c.standalone.is_some());
        }
    }
    // the target's tags are the source's tags
    let tt: Vec<&Inline> = d.units[2].segments[0].target.iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    assert_eq!(tt.len(), 2);
    // paragraph mode gives the same units
    let p = Mo.extract(&mo_le(), &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    assert_eq!(texts(&p), texts(&d));
}

#[test]
fn ids_survive_a_rebuild() {
    // Untranslated entries leave the file when it is rebuilt; the ids of the rest are table positions, so a file
    // that kept them keeps its ids.
    let src = pack(&pairs(&[("", HEADER_FR), ("A", "a"), ("B", "b"), ("C", "c")]));
    let mut d = ex(&src);
    set(&mut d, "e3", "bb");
    let out = merged(&d);
    let again = ex(&out);
    assert_eq!(again.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["e2", "e3", "e4"]);
    assert_eq!(Gettext::parse(&out).gettext("B"), "bb");
}

#[test]
fn every_fixture_round_trips() {
    for (name, data) in all_fixtures() {
        let d = ex(&data);
        // untouched merge is the original bytes (encoding, byte order, hash table kept)
        assert_eq!(merged(&d), data, "{name}: untouched merge");
        let n = d.units.len();
        if n == 0 {
            continue;
        }
        // pseudo-translated merge reads back with the same units, the same tags, and the translations
        let mut p = d.clone();
        pseudo(&mut p);
        let out = merged(&p);
        let back = ex(&out);
        assert_eq!(back.units.len(), n, "{name}");
        assert_eq!(texts(&back), texts(&d), "{name}");
        for (a, b) in back.units.iter().zip(&p.units) {
            assert_eq!(tags(a), tags(b), "{name}: tags");
            let t = flat_target(a);
            assert!(t.starts_with('\u{ab}') && t.ends_with('\u{bb}'), "{name}: {t:?}");
        }
        // merge is deterministic
        assert_eq!(merged(&p), out, "{name}");
    }
}

fn flat_target(u: &Unit) -> String {
    u.segments[0]
        .target
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

#[test]
fn nothing_translated_returns_the_original_bytes() {
    for (name, data) in all_fixtures() {
        // a target language alone changes nothing
        assert_eq!(merged(&ex_to(&data, "de-DE")), data, "{name}");
    }
}

#[test]
fn target_locale_merge_writes_language_and_plural_forms() {
    for (lang, tag, nplurals) in [("de", "de-DE", 2), ("ja", "ja", 1), ("ar", "ar-EG", 6), ("ru", "ru", 3), ("pl", "pl-PL", 3), ("pt", "pt-BR", 2)] {
        let mut d = ex_to(&fixture("lang_de.mo"), tag);
        pseudo(&mut d);
        let t = Gettext::parse(&merged(&d));
        let want = tag.replace('-', "_");
        assert_eq!(t.info("language").unwrap(), want, "{lang}");
        assert!(t.info("plural-forms").unwrap().starts_with(&format!("nplurals={nplurals};")), "{lang}: {:?}", t.info("plural-forms"));
    }
}

/// The eleven language fixtures: each catalog read in its charset, translated into its own language, every plural
/// slot of its rule filled with text.
#[test]
fn eleven_languages() {
    let langs = [
        ("de", "Hallo, willkommen zur\u{fc}ck"),
        ("fr", "Bonjour, bienvenue \u{e0} nouveau"),
        ("es", "Hola, bienvenido de nuevo"),
        ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{639}\u{648}\u{62f}\u{62a}\u{643}"),
        ("ja", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{3001}\u{304a}\u{5e30}\u{308a}\u{306a}\u{3055}\u{3044}"),
        ("zh", "\u{4f60}\u{597d},\u{6b22}\u{8fce}\u{56de}\u{6765}"),
        ("ko", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}, \u{b2e4}\u{c2dc} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}"),
        ("ru", "\u{417}\u{434}\u{440}\u{430}\u{432}\u{441}\u{442}\u{432}\u{443}\u{439}\u{442}\u{435}, \u{441} \u{432}\u{43e}\u{437}\u{432}\u{440}\u{430}\u{449}\u{435}\u{43d}\u{438}\u{435}\u{43c}"),
        ("pt", "Ol\u{e1}, bem-vindo de volta"),
        ("tr", "Merhaba, tekrar ho\u{15f} geldiniz"),
        ("pl", "Witaj, mi\u{142}o ci\u{119} znowu widzie\u{107}"),
    ];
    for (lang, hello) in langs {
        let data = fixture(&format!("lang_{lang}.mo"));
        let d = ex(&data);
        assert_eq!(texts(&d), ["%d file", "%d files", "Welcome back to the app", "Open"], "{lang}");
        assert_eq!(plain_text(&d.units[2].segments[0].target), hello, "{lang}: existing translation read in its charset");
        assert_eq!(merged(&d), data, "{lang}: untouched");
        // Translate into the language itself and merge with the language as the target.
        let mut t = ex_to(&data, lang);
        set(&mut t, "e2.0", "Eins %d");
        set(&mut t, "e2.1", "Viele %d");
        set(&mut t, "e3", hello);
        let out = merged(&t);
        let g = Gettext::parse(&out);
        assert_eq!(g.info("language").unwrap(), lang);
        assert_eq!(g.gettext("Welcome back to the app"), hello, "{lang}");
        let plural = g.info("plural-forms").unwrap();
        let np: usize = plural.trim_start_matches("nplurals=").split(';').next().unwrap().parse().unwrap();
        for n in 0..1200 {
            let idx = g.plural_index(n);
            assert!(idx < np, "{lang}: n={n} picks slot {idx} of {np}");
            let s = g.ngettext("%d file", "%d files", n);
            // the new translation, or the catalog's own text for a slot it already had (it is written for this language)
            assert!(!s.is_empty() && s != "%d files" && s != "%d file", "{lang}: n={n} gives {s:?}");
        }
        // an untranslated slot never shows the English msgid_plural: the singular goes to the n=1 slot, the plural to the rest
        assert_eq!(g.ngettext("%d file", "%d files", 1), if np == 1 { "Viele %d" } else { "Eins %d" }, "{lang}");
        assert_eq!(texts(&ex(&out)), texts(&d), "{lang}: reads back");
    }
}

#[test]
fn legacy_code_pages() {
    let cases = [
        ("legacy_ja_shift_jis.mo", "\u{30d5}\u{30a1}\u{30a4}\u{30eb}\u{3092}\u{958b}\u{304f}", "shift_jis"),
        ("legacy_zh_gbk.mo", "\u{6253}\u{5f00}\u{6587}\u{4ef6}", "gbk"),
        ("legacy_zh-tw_big5.mo", "\u{958b}\u{555f}\u{6a94}\u{6848}", "big5"),
        ("legacy_ko_euc_kr.mo", "\u{d30c}\u{c77c} \u{c5f4}\u{ae30}", "euc-kr"),
        ("legacy_ru_cp1251.mo", "\u{41e}\u{442}\u{43a}\u{440}\u{44b}\u{442}\u{44c} \u{444}\u{430}\u{439}\u{43b}", "windows-1251"),
        ("legacy_ar_cp1256.mo", "\u{641}\u{62a}\u{62d} \u{627}\u{644}\u{645}\u{644}\u{641}", "windows-1256"),
        ("legacy_tr_cp1254.mo", "Dosyay\u{131} a\u{e7}", "windows-1254"),
    ];
    for (name, text, label) in cases {
        let data = fixture(name);
        let d = ex(&data);
        assert_eq!(texts(&d), ["Open file"], "{name}");
        assert_eq!(plain_text(&d.units[0].segments[0].target), text, "{name}");
        assert_eq!(merged(&d), data, "{name}: untouched");
        // Translation that fits the code page: the file keeps its charset, label and bytes.
        let mut t = ex(&data);
        set(&mut t, "e2", text);
        let same_text = merged(&t);
        assert_eq!(same_text, data, "{name}: same text is no change");
        let enc = encoding_rs_for(label);
        let mut t = ex(&data);
        let changed = format!("{text}{text}");
        set(&mut t, "e2", &changed);
        let out = merged(&t);
        let g = Gettext::parse(&out);
        assert_eq!(g.charset.to_ascii_lowercase(), label.to_ascii_lowercase(), "{name}");
        let (bytes, _, bad) = enc.encode(&changed);
        assert!(!bad);
        assert_eq!(g.messages[&b"Open file"[..]], bytes.into_owned(), "{name}");
        assert_eq!(g.raw("Open file").unwrap(), changed, "{name}");
        // Text outside the code page: the file becomes UTF-8 and the header says so.
        let mut t = ex(&data);
        set(&mut t, "e2", "\u{1f600} emoji \u{2603}");
        let out = merged(&t);
        let g = Gettext::parse(&out);
        assert_eq!(g.charset.to_ascii_lowercase(), "utf-8", "{name}");
        assert_eq!(g.gettext("Open file"), "\u{1f600} emoji \u{2603}");
    }
}

fn encoding_rs_for(label: &str) -> &'static bilingua_model::encoding_rs::Encoding {
    bilingua_model::encoding_rs::Encoding::for_label(label.as_bytes()).unwrap()
}

#[test]
fn header_shapes() {
    // no header entry at all: a target merge adds one that names the language and the UTF-8 charset
    let data = fixture("no_header.mo");
    let mut d = ex_to(&data, "de-DE");
    set(&mut d, "e1", "\u{d6}ffnen");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.info("language").unwrap(), "de_DE");
    assert_eq!(g.charset, "utf-8");
    assert!(g.has("") && g.gettext("Open") == "\u{d6}ffnen");
    // ... and without a target language the header stays absent
    let mut d = ex(&data);
    set(&mut d, "e1", "\u{d6}ffnen");
    assert!(!Gettext::parse(&merged(&d)).has(""));
    // the template word CHARSET is read as UTF-8, and written as UTF-8
    let data = fixture("charset_placeholder.mo");
    let mut d = ex(&data);
    set(&mut d, "e2", "Si");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.charset, "utf-8");
    // an unknown language keeps the header as it is
    let mut d = ex_to(&fixture("catalog_le.mo"), "tlh");
    set(&mut d, "e3", "x");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.info("language").unwrap(), "fr");
    assert_eq!(g.info("plural-forms").unwrap(), "nplurals=2; plural=(n > 1);");
    // an unusable language tag is ignored
    let mut d = ex_to(&fixture("catalog_le.mo"), "not a tag!");
    set(&mut d, "e3", "x");
    assert_eq!(Gettext::parse(&merged(&d)).info("language").unwrap(), "fr");
    // a header-only catalog has no units and comes back as it was
    let d = ex(&fixture("empty_header_only.mo"));
    assert!(d.units.is_empty());
    assert_eq!(merged(&d), fixture("empty_header_only.mo"));
}

#[test]
fn keeps_slots_of_a_catalog_already_in_the_target_language() {
    // A Russian catalog re-exported to Russian: the slots past the two source forms are the translator's own.
    let ru = pack(&pairs(&[
        ("", "Language: ru\nContent-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);\n"),
        ("%d file\0%d files", "%d \u{444}\u{430}\u{439}\u{43b}\0%d \u{444}\u{430}\u{439}\u{43b}\u{430}\0%d \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}"),
    ]));
    let mut d = ex_to(&ru, "ru-RU");
    set(&mut d, "e2.0", "%d NEW-one");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.ngettext("%d file", "%d files", 1), "%d NEW-one");
    assert_eq!(g.ngettext("%d file", "%d files", 3), "%d \u{444}\u{430}\u{439}\u{43b}\u{430}");
    assert_eq!(g.ngettext("%d file", "%d files", 5), "%d \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}");
    // A French catalog translated to Russian: the French extra text is not carried over.
    let mut d = ex_to(&mo_le(), "ru-RU");
    set(&mut d, "e2.0", "%d a");
    set(&mut d, "e2.1", "%d b");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.ngettext("%d file", "%d files", 2), "%d b");
    assert_eq!(g.ngettext("%d file", "%d files", 5), "%d b");
}

#[test]
fn duplicate_keys_and_headers() {
    // msgfmt reports a duplicate; the first one wins when the catalog is rebuilt.
    let src = pack(&pairs(&[("", HEADER_FR), ("Dup", "one"), ("Dup", "two"), ("Other", "x")]));
    let mut d = ex(&src);
    assert_eq!(texts(&d), ["Dup", "Dup", "Other"]);
    set(&mut d, "e4", "X");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.gettext("Dup"), "one");
    // two header entries: the real one (made of header fields) is kept
    let src = pack(&pairs(&[("", "stray"), ("", HEADER_FR), ("A", "a")]));
    let mut d = ex(&src);
    assert_eq!(texts(&d), ["A"]);
    set(&mut d, "e3", "b");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.info("language").unwrap(), "fr");
    assert_eq!(g.gettext("A"), "b");
    // same text, two contexts, and the empty context: three separate units
    let src = pack(&pairs(&[("", HEADER_FR), ("\u{4}A", "empty"), ("m\u{4}A", "m"), ("A", "none")]));
    let d = ex(&src);
    assert_eq!(d.units.len(), 3);
    assert_eq!(d.units[0].context.key.as_deref(), Some(" :: A"));
    assert_eq!(merged(&d), src);
}

#[test]
fn line_break_rule_follows_the_msgid() {
    // gettext's own rule (msgfmt -c): a msgstr begins and ends with a newline exactly when its msgid does.
    let src = pack(&pairs(&[("", HEADER_FR), ("Line\n", "x"), ("Plain", "y"), ("\nLead", "z")]));
    let mut d = ex(&src);
    set(&mut d, "e2", "Zeile");
    set(&mut d, "e3", "Eben\n");
    set(&mut d, "e4", "Anfang");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.gettext("Line\n"), "Zeile\n");
    assert_eq!(g.gettext("Plain"), "Eben");
    assert_eq!(g.gettext("\nLead"), "\nAnfang");
}

#[test]
fn plural_with_an_empty_plural_form() {
    // `msgid_plural ""`: one unit (the singular); the translation goes to slot 0 of the plural entry.
    let src = pack(&[(b"".to_vec(), HEADER_FR.as_bytes().to_vec()), (b"Thing\0".to_vec(), b"Chose\0Choses".to_vec())]);
    let mut d = ex(&src);
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].id, "e2");
    set(&mut d, "e2", "Truc");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.messages[&b"Thing\0"[..]], b"Truc\0Choses");
}

#[test]
fn languages_that_fill_every_slot() {
    // Arabic: six slots; the singular goes to slot 1 ("one"), the plural to every other slot.
    let src = pack(&pairs(&[("", "Language: en\nContent-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=2; plural=(n != 1);\n"), ("One\0Many", "\0")]));
    let mut d = ex_to(&src, "ar");
    set(&mut d, "e2.0", "S");
    set(&mut d, "e2.1", "P");
    let g = Gettext::parse(&merged(&d));
    let slots: Vec<String> = g.text(&g.messages[&b"One\0Many"[..]]).split('\0').map(str::to_string).collect();
    assert_eq!(slots, ["P", "S", "P", "P", "P", "P"]);
    // Polish: slot 0 is the singular.
    let mut d = ex_to(&src, "pl");
    set(&mut d, "e2.0", "S");
    set(&mut d, "e2.1", "P");
    let g = Gettext::parse(&merged(&d));
    assert_eq!(g.text(&g.messages[&b"One\0Many"[..]]), "S\0P\0P");
}

#[test]
fn tampered_documents_are_typed_errors() {
    let d = ex(&mo_le());
    let mut text = d.clone();
    text.skeleton = Skeleton::Text { parts: vec![], encoding: "utf-8".into(), parser: None };
    assert!(matches!(Mo.merge(&text), Err(Error::Write(_))));
    let mut empty = d.clone();
    empty.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(Mo.merge(&empty), Err(Error::Write(_))));
    let mut junk = d.clone();
    junk.skeleton = Skeleton::Zip { entries: vec![ZipEntry::raw("catalog.mo", b"junk".to_vec())], parser: None };
    assert!(Mo.merge(&junk).is_err());
    let mut other = d.clone();
    other.skeleton = Skeleton::Zip { entries: vec![], parser: Some(ParserInfo::new("bilingua-po", "0.1.0", 1)) };
    assert!(matches!(Mo.merge(&other), Err(Error::ParserVersion { .. })));
    // a unit nobody knows is ignored
    let mut extra = d.clone();
    extra.units.push(Unit::new("e99", vec![Inline::text("ghost")]));
    assert_eq!(merged(&extra), mo_le());
}

#[test]
fn a_catalog_in_a_code_page_outside_the_short_list_is_read() {
    // ISO-8859-13 (Baltic): byte 0xFE is a z with a caron
    let header = b"Language: lt\nContent-Type: text/plain; charset=iso-8859-13\n".to_vec();
    let data = pack(&[(Vec::new(), header), (b"Open file".to_vec(), vec![b'a', 0xFE])]);
    let d = ex(&data);
    assert_eq!(texts(&d), ["Open file"]);
    assert_eq!(plain_text(&d.units[0].segments[0].target), "a\u{17e}");
    assert_eq!(merged(&d), data);
    // a translation that fits keeps the code page; one that does not makes the file UTF-8, as for the short list
    let mut t = ex(&data);
    set(&mut t, "e2", "\u{17e}\u{161}");
    let g = Gettext::parse(&merged(&t));
    assert_eq!(g.charset.to_ascii_lowercase(), "iso-8859-13");
    assert_eq!(g.messages[&b"Open file"[..]], vec![0xFE, 0xF0]);
    let mut t = ex(&data);
    set(&mut t, "e2", "\u{65e5}\u{672c}");
    let g = Gettext::parse(&merged(&t));
    assert_eq!(g.charset.to_ascii_lowercase(), "utf-8");
    assert_eq!(g.gettext("Open file"), "\u{65e5}\u{672c}");
}
