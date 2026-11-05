//! The Localization OS suite `tests/verify_xcloc_filter.py`. The Python file is one script with four sections
//! (parse, round trip, localized, hostile) plus the registration and mime checks; each is a test here (5 of 5),
//! and every assertion of the script is made.
#[path = "xcloc_common/mod.rs"]
mod xcloc_common;
use bilingua_model::*;
use bilingua_xcstrings::Xcloc;
use bilingua_zip::{write_zip, EntryMeta};
use xcloc_common::*;

#[track_caller]
fn err(r: Result<Document, Error>) -> Error {
    match r {
        Err(e) => e,
        Ok(d) => panic!("must be refused, got {} units", d.units.len()),
    }
}

fn msg(e: &Error) -> String {
    e.to_string()
}

fn lk(e: &Error) -> LimitKind {
    match e {
        Error::Limit(l) => l.kind,
        o => panic!("expected a limit error, got {o}"),
    }
}

#[test]
fn registered() {
    assert_eq!(Xcloc.id(), "xcloc");
    assert_eq!(Xcloc.extensions(), &["xcloc"]);
    assert!(Xcloc.sniff(&Bundle::default().build()));
    assert!(!Xcloc.sniff(XLIFF));
}

#[test]
fn parse() {
    let src = Bundle::default().build();
    let d = ex(&src);
    let texts: Vec<String> = d.units.iter().map(text).collect();
    assert_eq!(&texts[..2], ["Hello %@", "%1$lld items in %2$@"]);
    // the empty <source/> is skipped (XLIFF filter rule)
    assert_eq!(texts.len(), 3, "{texts:?}");
    // <g> became a paired code
    assert!(texts[2].contains("\u{27E6}g1\u{27E7}") && texts[2].contains("\u{27E6}/g1\u{27E7}"), "{}", texts[2]);
    assert_eq!(d.source_lang.as_deref(), Some("en"));
    // bundle at the archive root
    let root = Bundle { prefix: "", macosx: false, ..Default::default() }.build();
    assert_eq!(ex(&root).units.iter().map(text).collect::<Vec<_>>(), texts);
    // two XLIFF files: both read, ids tell them apart
    let two = Bundle { xliffs: vec![("fr.xliff", XLIFF), ("fr-extra.xliff", XLIFF2)], ..Default::default() }.build();
    let d2 = ex(&two);
    assert_eq!(d2.units.len(), 4);
    assert_eq!(text(&d2.units[3]), "My App");
    let ids: std::collections::HashSet<&str> = d2.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids.len(), 4);
    assert!(d2.units[3].id.starts_with("x2/"));
}

#[test]
fn round_trip() {
    let src = Bundle::default().build();
    assert_eq!(merge(&ex(&src)), src, "no-change merge must return the original bytes");
    let mut d = ex(&src);
    let tok = text(&d.units[2]);
    let fr_bold = tok.replace("Tap", "Touchez").replace("now", "maintenant").replace("Save", "Enregistrer");
    put(&mut d, 0, "Bonjour %@");
    put(&mut d, 1, "%1$lld articles dans %2$@");
    put(&mut d, 2, &fr_bold);
    let out = merge(&d);
    let (before, after) = (members(&src), members(&out));
    assert_eq!(before.iter().map(|m| &m.0).collect::<Vec<_>>(), after.iter().map(|m| &m.0).collect::<Vec<_>>(), "member order or names changed");
    for (b, a) in before.iter().zip(&after) {
        assert_eq!((b.1, b.2), (a.1, a.2), "{}: timestamp or compression changed", b.0);
        if !b.0.ends_with("fr.xliff") {
            assert_eq!(b.3, a.3, "{} bytes changed", b.0);
        }
    }
    assert_eq!(comment(&out), b"made by hand");
    let x = String::from_utf8(member(&out, "fr.xcloc/Localized Contents/fr.xliff")).unwrap();
    assert!(x.contains("<target state=\"translated\">Bonjour %@</target>"), "{x}");
    assert!(x.contains("%1$lld articles dans %2$@"));
    assert!(x.contains("<g id=\"1\">Enregistrer</g>") && x.contains("Touchez"), "{x}");
    assert!(x.contains("<note>Greeting on the home screen</note>"));
    assert!(x.contains("target-language=\"fr\""), "a plain merge never touches the language");
    // re-reading the output gives the same source units
    assert_eq!(ex(&out).units.iter().map(text).collect::<Vec<_>>(), d.units.iter().map(text).collect::<Vec<_>>());
    // the second XLIFF is reached through its own ids
    let two = Bundle { xliffs: vec![("fr.xliff", XLIFF), ("fr-extra.xliff", XLIFF2)], ..Default::default() }.build();
    let mut d = ex(&two);
    put(&mut d, 3, "Mon appli");
    let out2 = merge(&d);
    let extra = member(&out2, "fr.xcloc/Localized Contents/fr-extra.xliff");
    let extra = String::from_utf8(extra).unwrap();
    // Difference from LocOS (which writes a bare <target>): the XLIFF writer gives a new target a state, and a
    // finished translation is "translated".
    assert!(extra.contains("<target state=\"translated\">Mon appli</target>"), "{extra}");
    assert_eq!(member(&out2, "fr.xcloc/Localized Contents/fr.xliff"), XLIFF);
}

#[test]
fn localized() {
    let src = Bundle::default().build();
    let mut d = ex_with(&src, Some("de-DE"));
    put(&mut d, 0, "Hallo %@");
    let out = merge(&d);
    let cj = member(&out, "fr.xcloc/contents.json");
    let want = String::from_utf8(CONTENTS.to_vec()).unwrap().replace("\"targetLocale\" : \"fr\"", "\"targetLocale\" : \"de-DE\"");
    assert_eq!(String::from_utf8(cj).unwrap(), want);
    let x = String::from_utf8(member(&out, "fr.xcloc/Localized Contents/fr.xliff")).unwrap();
    assert!(x.contains("target-language=\"de-DE\"") && x.contains("Hallo %@"), "{x}");
    // same language as the catalog: contents.json untouched
    let same = merge(&ex_with(&src, Some("fr")));
    assert_eq!(same, src);
    let d = ex_with(&src, Some("fr"));
    assert_eq!(member(&merge(&d), "fr.xcloc/contents.json"), CONTENTS);
}

#[test]
fn hostile() {
    let opts = ExtractOptions::default();
    let e = err(Xcloc.extract(b"not a zip at all", &opts));
    assert!(msg(&e).contains("not a zipped Xcode"), "{e}");
    let e = err(Xcloc.extract(XLIFF, &opts));
    assert!(msg(&e).contains("not a zipped Xcode"), "{e}");

    let m = |name: &str, body: &[u8]| (EntryMeta { name: name.into(), stored: false, dir: false, modified: Some([2024, 1, 2, 3, 4, 6]) }, body.to_vec());
    let no_contents = write_zip(&[m("fr.xcloc/Localized Contents/fr.xliff", XLIFF)]).unwrap();
    let e = err(Xcloc.extract(&no_contents, &opts));
    assert!(msg(&e).contains("no contents.json"), "{e}");

    let twice = write_zip(&[
        m("fr.xcloc/contents.json", CONTENTS),
        m("fr.xcloc/Localized Contents/fr.xliff", XLIFF),
        m("de.xcloc/contents.json", CONTENTS),
        m("de.xcloc/Localized Contents/fr.xliff", XLIFF),
    ])
    .unwrap();
    let e = err(Xcloc.extract(&twice, &opts));
    assert!(msg(&e).contains("2 localization catalogs"), "{e}");

    // the same member name twice (two names of equal length, then one patched to the other)
    let mut dup = write_zip(&[m("contents.json", CONTENTS), m("Localized Contents/fr.xliff", XLIFF), m("Localized Contents/fx.xliff", XLIFF)]).unwrap();
    let (from, to) = (b"Localized Contents/fx.xliff", b"Localized Contents/fr.xliff");
    let mut i = 0;
    while i + from.len() <= dup.len() {
        if &dup[i..i + from.len()] == from {
            dup[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
    let e = err(Xcloc.extract(&dup, &opts));
    assert!(matches!(e, Error::Parse { .. }), "{e}");

    let e = err(Xcloc.extract(&Bundle { contents: b"[1, 2]", ..Default::default() }.build(), &opts));
    assert!(msg(&e).contains("not a JSON object"), "{e}");
    let e = err(Xcloc.extract(&Bundle { contents: b"{nope", ..Default::default() }.build(), &opts));
    assert!(msg(&e).contains("not valid JSON"), "{e}");

    // a contents.json over 1 MB (hex digits from a counter, so the zip ratio guard does not fire first)
    let mut big = b"{\"targetLocale\": \"fr\", \"pad\": \"".to_vec();
    let mut x: u32 = 12345;
    while big.len() < 1_100_000 {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        big.extend_from_slice(format!("{:08x}", x).as_bytes());
    }
    big.extend_from_slice(b"\"}");
    let e = err(Xcloc.extract(&Bundle { contents: &big, ..Default::default() }.build(), &opts));
    assert!(msg(&e).contains("contents.json over"), "{e}");

    // DOCTYPE with an entity in the XLIFF
    let evil = String::from_utf8(XLIFF.to_vec())
        .unwrap()
        .replace("<?xml version=\"1.0\" encoding=\"UTF-8\"?>", "<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/passwd\">]>");
    let e = err(Xcloc.extract(&Bundle { xliffs: vec![("fr.xliff", evil.as_bytes())], ..Default::default() }.build(), &opts));
    assert_eq!(lk(&e), LimitKind::XmlEntities);

    // a zip bomb member
    let bomb = write_zip(&[m("contents.json", CONTENTS), m("Localized Contents/fr.xliff", XLIFF), m("Source Contents/zeros.bin", &vec![0u8; 8 * 1024 * 1024])]).unwrap();
    let e = err(Xcloc.extract(&bomb, &opts));
    assert_eq!(lk(&e), LimitKind::ZipRatio);
    assert!(msg(&e).contains("compression bomb"), "{e}");

    // segment cap
    let units: String = (0..30).map(|i| format!("<trans-unit id=\"u{i}\"><source>String {i}</source></trans-unit>")).collect();
    let many = String::from_utf8(XLIFF2.to_vec()).unwrap().replace("<source>My App</source>", &format!("<source>My App</source>{units}"));
    let small = ExtractOptions { limits: Some(Limits { max_units: 20, ..Limits::default() }), ..Default::default() };
    let e = err(Xcloc.extract(&Bundle { xliffs: vec![("fr.xliff", many.as_bytes())], ..Default::default() }.build(), &small));
    assert_eq!(lk(&e), LimitKind::Units);
}
