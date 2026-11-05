//! The Localization OS suite `tests/verify_mo_filter.py`, ported: all 11 test functions are covered. Nine are
//! ported whole; `test_build_po` keeps its unit half (the PO export is another layer); `test_export_surfaces`
//! (HTTP download and CLI export) is not a filter test and is not ported.
//!
//! Where LocOS always writes UTF-8, this crate keeps the file's own encoding when every character fits (B6): the
//! charset test below says so. Everything else is checked against the same expectations as the Python suite, with
//! an independent gettext reader (`common::Gettext`) standing in for CPython's `gettext` and babel.

mod common;
use bilingua_model::*;
use bilingua_mo::Mo;
use common::*;

const PO_EQUIVALENT: &str = "msgid \"\"\nmsgstr \"\"\n\"Project-Id-Version: demo 1.0\\n\"\n\"Language: fr\\n\"\n\"MIME-Version: 1.0\\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\"Content-Transfer-Encoding: 8bit\\n\"\n\"Plural-Forms: nplurals=2; plural=(n > 1);\\n\"\n\nmsgid \"%d file\"\nmsgid_plural \"%d files\"\nmsgstr[0] \"%d fichier\"\nmsgstr[1] \"%d fichiers\"\n\nmsgid \"Hello %(name)s, you have {0} messages\"\nmsgstr \"Bonjour %(name)s, vous avez {0} messages\"\n\nmsgctxt \"menu\"\nmsgid \"Open\"\nmsgstr \"Ouvrir\"\n\nmsgctxt \"state\"\nmsgid \"Open\"\nmsgstr \"Ouvert\"\n";

fn translated(data: &[u8], locale: Option<&str>) -> Vec<u8> {
    let mut d = match locale {
        Some(l) => ex_to(data, l),
        None => ex(data),
    };
    set(&mut d, "e2.0", "%d Datei");
    set(&mut d, "e2.1", "%d Dateien");
    set(&mut d, "e3", "Hallo %(name)s, Sie haben {0} Nachrichten");
    set(&mut d, "e4", "Oeffnen");
    set(&mut d, "e5", "Offen");
    merged(&d)
}

fn fill(s: String, n: i64) -> String {
    s.replace("%d", &n.to_string())
}

#[test]
fn registered() {
    assert_eq!(Mo.id(), "mo");
    assert_eq!(Mo.extensions(), &["mo", "gmo"]);
    assert!(Mo.sniff(&mo_le()) && Mo.sniff(&mo_be_hash()));
    assert!(!Mo.sniff(b"msgid \"\"\nmsgstr \"\"\n") && !Mo.sniff(&[0u8; 40]));
}

#[test]
fn units_match_po() {
    let po = bilingua_po::Po
        .extract(PO_EQUIVALENT.as_bytes(), &ExtractOptions { target_lang: Some("fr".into()), ..Default::default() })
        .unwrap();
    let want: Vec<String> = po.units.iter().map(flat).collect();
    for (name, data) in [("LE", mo_le()), ("BE+hash", mo_be_hash())] {
        let got = ex(&data);
        assert_eq!(texts(&got), want, "{name}");
        assert_eq!(got.format, "mo");
    }
    assert_eq!(want, ["%d file", "%d files", "Hello %(name)s, you have {0} messages", "Open", "Open"]);
}

#[test]
fn identity() {
    for data in [mo_le(), mo_be_hash()] {
        let d = ex(&data);
        assert_eq!(merged(&d), data);
        // Through JSON too: the skeleton carries the file.
        let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(merged(&back), data);
        // Supplying the source text IS a change (the msgstrs become that text), so it recompiles.
        let mut same = ex(&data);
        let all: Vec<(String, String)> = same.units.iter().map(|u| (u.id.clone(), flat(u))).collect();
        for (id, t) in all {
            set(&mut same, &id, &t);
        }
        assert_ne!(merged(&same), data);
    }
}

#[test]
fn translated_build_is_valid_mo() {
    for data in [mo_le(), mo_be_hash()] {
        let out = translated(&data, None);
        let t = Gettext::parse(&out);
        assert_eq!(t.gettext("Hello %(name)s, you have {0} messages"), "Hallo %(name)s, Sie haben {0} Nachrichten");
        assert_eq!(t.pgettext("menu", "Open"), "Oeffnen");
        assert_eq!(t.pgettext("state", "Open"), "Offen");
        assert_eq!(t.ngettext("%d file", "%d files", 1), "%d Datei");
        assert_eq!(t.ngettext("%d file", "%d files", 5), "%d Dateien");
        // Re-parsing our own output gives the same units (extract and merge walk one order).
        assert_eq!(texts(&ex(&out)), texts(&ex(&data)));
        // Little-endian, revision 0, no hash table, originals sorted bytewise.
        assert_eq!(&out[..8], &[0xde, 0x12, 0x04, 0x95, 0, 0, 0, 0]);
        let keys: Vec<&Vec<u8>> = {
            let mut k: Vec<&Vec<u8>> = t.messages.keys().collect();
            k.sort();
            k
        };
        assert_eq!(keys.len(), 5);
    }
    // An untranslated entry (empty msgstr) is left out, as msgfmt does.
    let sparse = pack(&pairs(&[("", HEADER_FR), ("Keep", "Garder"), ("Skip", "")]));
    let mut d = ex(&sparse);
    set(&mut d, "e2", "Behalten");
    let out = merged(&d);
    let t = Gettext::parse(&out);
    assert_eq!(t.gettext("Keep"), "Behalten");
    assert_eq!(t.gettext("Skip"), "Skip");
    assert!(!out.windows(4).any(|w| w == b"Skip"));
}

#[test]
fn localized_plural_forms() {
    let out = translated(&mo_le(), Some("ru-RU"));
    let t = Gettext::parse(&out);
    assert_eq!(t.info("language").unwrap(), "ru_RU");
    assert!(t.info("plural-forms").unwrap().starts_with("nplurals=3;"));
    // #729 (decision 18): slot 2 (Russian "many") holds the plural translation.
    for n in [0, 5, 11, 100] {
        assert_eq!(t.ngettext("%d file", "%d files", n), "%d Dateien", "{n}");
    }
    assert_eq!(t.ngettext("%d file", "%d files", 1), "%d Datei");
    // A header with no Language field gets one for the target.
    let bare = pack(&pairs(&[
        ("", "Content-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=2; plural=(n != 1);\n"),
        ("A\0As", "x\0xs"),
    ]));
    let mut d = ex_to(&bare, "pl-PL");
    set(&mut d, "e2.0", "Un");
    set(&mut d, "e2.1", "Uns");
    let t = Gettext::parse(&merged(&d));
    assert_eq!(t.info("language").unwrap(), "pl_PL");
    assert!(t.info("plural-forms").unwrap().starts_with("nplurals=3;"));
}

#[test]
fn one_form_target_uses_plural_682() {
    let src = pack(&[
        (b"".to_vec(), b"Language: en\nContent-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=2; plural=(n != 1);\n".to_vec()),
        (b"One file\0%d files".to_vec(), b"\0".to_vec()),
        (b"Hello".to_vec(), b"".to_vec()),
    ]);
    for locale in ["ja-JP", "tr-TR", "zh-CN", "ko-KR", "vi-VN", "th-TH"] {
        let mut d = ex_to(&src, locale);
        set(&mut d, "e2.0", "T-one");
        set(&mut d, "e2.1", "T-many %d");
        set(&mut d, "e3", "T-hello");
        let t = Gettext::parse(&merged(&d));
        assert!(t.info("plural-forms").unwrap().starts_with("nplurals=1;"), "{locale}");
        for n in [0, 1, 5] {
            assert_eq!(fill(t.ngettext("One file", "%d files", n), n), format!("T-many {n}"), "{locale} {n}");
        }
        assert_eq!(t.gettext("Hello"), "T-hello");
        // only the singular translated: it is all there is, keep it
        let mut d = ex_to(&src, locale);
        set(&mut d, "e2.0", "T-one");
        let t = Gettext::parse(&merged(&d));
        assert_eq!(t.ngettext("One file", "%d files", 5), "T-one", "{locale}");
        // a plural unit left without a translation is not one and never replaces the translated singular
        let mut d = ex_to(&src, locale);
        set(&mut d, "e2.0", "T-one");
        set(&mut d, "e3", "T-hello");
        let t = Gettext::parse(&merged(&d));
        assert_eq!(t.ngettext("One file", "%d files", 5), "T-one", "{locale}");
    }
    // two-form targets are unchanged
    let mut d = ex_to(&src, "de-DE");
    set(&mut d, "e2.0", "Eine Datei");
    set(&mut d, "e2.1", "%d Dateien");
    let t = Gettext::parse(&merged(&d));
    assert_eq!(t.ngettext("One file", "%d files", 1), "Eine Datei");
    assert_eq!(fill(t.ngettext("One file", "%d files", 5), 5), "5 Dateien");
}

#[test]
fn cldr_only_targets_739() {
    let src = pack(&pairs(&[
        ("", "Language: en\nContent-Type: text/plain; charset=UTF-8\nPlural-Forms: nplurals=2; plural=(n != 1);\n"),
        ("One file\0%d files", "\0"),
    ]));
    let build = |locale: &str, one: &str, many: &str| {
        let mut d = ex_to(&src, locale);
        set(&mut d, "e2.0", one);
        set(&mut d, "e2.1", many);
        Gettext::parse(&merged(&d))
    };
    let t = build("id-ID", "Satu berkas", "%d berkas");
    assert_eq!(t.info("language").unwrap(), "id_ID");
    assert_eq!(t.info("plural-forms").unwrap(), "nplurals=1; plural=0;");
    for n in [0, 1, 5] {
        assert_eq!(fill(t.ngettext("One file", "%d files", n), n), format!("{n} berkas"));
    }
    let t = build("sw-KE", "Faili moja", "Faili %d");
    assert_eq!(t.info("language").unwrap(), "sw_KE");
    assert_eq!(t.ngettext("One file", "%d files", 1), "Faili moja");
    assert_eq!(fill(t.ngettext("One file", "%d files", 5), 5), "Faili 5");
    // the PO half of the Python test: the header names the language and keeps the two-form rule
    let t = build("ta-IN", "x", "y");
    assert_eq!(t.info("language").unwrap(), "ta_IN");
    assert_eq!(t.info("plural-forms").unwrap(), "nplurals=2; plural=(n != 1);");
}

#[test]
fn build_po_units_and_header() {
    // The unit half of `test_build_po`: the PO of the same catalog has the same units; the target header is
    // stamped. (The PO text export itself is a LocOS service, not a filter.)
    let mut d = ex_to(&mo_le(), "es-ES");
    set(&mut d, "e3", "Hola %(name)s, tienes {0} mensajes");
    let out = merged(&d);
    let t = Gettext::parse(&out);
    assert_eq!(t.gettext("Hello %(name)s, you have {0} messages"), "Hola %(name)s, tienes {0} mensajes");
    assert!(t.has("menu\u{4}Open") && t.has("%d file\0%d files"));
    assert_eq!(t.info("language").unwrap(), "es_ES");
    assert_eq!(texts(&ex(&out)), texts(&ex(&mo_le())));
}

#[test]
fn declared_charset() {
    let hdr = "Content-Type: text/plain; charset=ISO-8859-1\nLanguage: fr\n";
    let data = pack(&[(b"".to_vec(), hdr.as_bytes().to_vec()), (b"Coffee".to_vec(), b"Caf\xe9".to_vec())]);
    let d = ex(&data);
    assert_eq!(texts(&d), ["Coffee"]);
    assert_eq!(plain_text(&d.units[0].segments[0].target), "Caf\u{e9}");
    assert_eq!(merged(&d), data);
    // LocOS writes UTF-8 here; this crate keeps ISO-8859-1 because every character fits (B6).
    let mut d = ex(&data);
    set(&mut d, "e2", "Caf\u{e9} cr\u{e8}me");
    let out = merged(&d);
    let t = Gettext::parse(&out);
    assert_eq!(t.charset, "iso-8859-1");
    assert_eq!(t.messages[&b"Coffee"[..]], b"Caf\xe9 cr\xe8me");
    // A translation that does not fit the code page makes the file UTF-8, and the header says so.
    let mut d = ex(&data);
    set(&mut d, "e2", "\u{41a}\u{43e}\u{444}\u{435}");
    let out = merged(&d);
    let t = Gettext::parse(&out);
    assert_eq!(t.charset, "utf-8");
    assert_eq!(t.gettext("Coffee"), "\u{41a}\u{43e}\u{444}\u{435}");
}

fn refused(data: &[u8], needle: &str) {
    let msg = match try_ex(data) {
        Err(e) => e.to_string().to_lowercase(),
        Ok(_) => panic!("must be refused: {:?}", &data[..data.len().min(32)]),
    };
    assert!(msg.contains(&needle.to_lowercase()), "{msg:?} should mention {needle:?}");
}

fn put(data: &[u8], at: usize, words: &[u32]) -> Vec<u8> {
    let mut v = data.to_vec();
    for (i, w) in words.iter().enumerate() {
        v[at + 4 * i..at + 4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    v
}

#[test]
fn hostile() {
    let le = mo_le();
    let len = le.len() as u32;
    refused(b"", "shorter");
    refused(&[0u8; 40], "magic");
    refused(b"msgid \"\"\nmsgstr \"\"\n", "shorter");
    refused(&le[..20], "shorter");
    let rev2 = pack_with(&catalog(), &Pack { revision: 2 << 16, ..Default::default() });
    refused(&rev2, "revision");
    // Original table pointing past the end of the file.
    refused(&put(&le, 12, &[len + 100]), "outside");
    // Hash table past the end.
    refused(&put(&le, 20, &[1000, len]), "hash table");
    // A string offset outside the file.
    refused(&put(&le, 28 + 8, &[5, len + 10]), "string");
    // Last string with no NUL terminator (ends exactly at EOF).
    refused(&le[..le.len() - 1], "string");
    // N far over the segment cap is refused before any table read.
    let e = try_ex(&put(&le, 8, &[0xFFFF_FFFF])).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::Units), "{e:?}");
    // N inside the cap but tables longer than the file.
    refused(&put(&le, 8, &[40000]), "outside");
    // System-dependent strings (revision 0.1 with a non-zero count).
    let sysdep = pack_with(&catalog(), &Pack { revision: 1, sysdep: Some([0, 0, 1, 0, 0]), ..Default::default() });
    refused(&sysdep, "system-dependent");
    // Revision 0.1 with NO sysdep strings reads fine.
    let ok = pack_with(&catalog(), &Pack { revision: 1, sysdep: Some([0; 5]), ..Default::default() });
    assert!(!texts(&ex(&ok)).is_empty());
    // Every entry pointing at one big string: decoded size far above the file size.
    let n = 300u32;
    let base = 28 + 16 * n;
    let mut bad = Vec::new();
    for w in [0x9504_12de, 0, n, 28, 28 + 8 * n, 0, 0] {
        bad.extend_from_slice(&u32::to_le_bytes(w));
    }
    for _ in 0..n {
        bad.extend_from_slice(&1u32.to_le_bytes());
        bad.extend_from_slice(&base.to_le_bytes());
    }
    for _ in 0..n {
        bad.extend_from_slice(&5000u32.to_le_bytes());
        bad.extend_from_slice(&(base + 2).to_le_bytes());
    }
    bad.extend_from_slice(b"A\0");
    bad.extend(std::iter::repeat_n(b'x', 5000));
    bad.push(0);
    let e = try_ex(&bad).unwrap_err();
    assert!(e.to_string().contains("overlapping"), "{e}");
    // NULs where no plural is declared.
    refused(&pack(&pairs(&[("", HEADER_FR), ("One", "a\0b")])), "NUL");
    refused(&pack(&pairs(&[("", HEADER_FR), ("a\0b\0c", "x")])), "NUL");
    // Bytes that are not the declared charset.
    refused(&pack(&[(b"".to_vec(), HEADER_FR.as_bytes().to_vec()), (b"\xff\xfe".to_vec(), b"x".to_vec())]), "UTF-8");
    // The unit cap applies while units are built.
    let lim = Limits { max_units: 3, ..Limits::default() };
    let e = Mo.extract(&le, &ExtractOptions { limits: Some(lim), ..Default::default() }).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::Units), "{e:?}");
}
