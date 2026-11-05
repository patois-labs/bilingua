//! Okapi's Visio sample, languages, encodings, comments, hostile input, a property test and LibreOffice.

use crate::fixtures::*;
use crate::tests::*;
use crate::*;
use proptest::prelude::*;

const OKAPI: &[u8] = include_bytes!("../tests/data/2-pages.vsdx");

pub const LANGS: [(&str, &str); 11] = [
    ("en", "Welcome to the show"),
    ("es", "Bienvenidos al espectáculo"),
    ("fr", "Bienvenue au spectacle"),
    ("it", "Benvenuti allo spettacolo"),
    ("de", "Willkommen zur Show"),
    ("nl", "Welkom bij de show"),
    ("pt", "Bem-vindos ao espetáculo"),
    ("ja", "ショーへようこそ"),
    ("zh", "欢迎观看演出"),
    ("ko", "쇼에 오신 것을 환영합니다"),
    ("ar", "مرحبا بكم في العرض"),
];

fn texts(doc: &Document) -> Vec<String> {
    doc.units.iter().map(|u| profile::tokens::to_locos_text(&u.source()).0).collect()
}

/// Every unit gets "[n] " in front of its first text; tags stay.
fn pseudo(doc: &mut Document) {
    for (n, u) in doc.units.clone().iter().enumerate() {
        let mut done = false;
        let t: Vec<Inline> = u
            .source()
            .into_iter()
            .map(|i| match i {
                Inline::Text(t) if !done => {
                    done = true;
                    Inline::text(format!("[{n}] {t}"))
                }
                o => o,
            })
            .collect();
        let t = if done { t } else { [vec![Inline::text(format!("[{n}] "))], t].concat() };
        doc.set_target(&u.segments[0].id, t).unwrap();
    }
}

#[test]
fn okapi_two_pages_matches_locos() {
    // The units LocOS makes of Okapi's own test resource (2-pages.vsdx).
    let doc = extract(OKAPI);
    let want = [
        "Page-1",
        "Page-2",
        "Paragraph ⟦x1⟧1.⟦x2⟧\n⟦x3⟧Pa⟦x4⟧ra⟦x5⟧gr⟦x6⟧aph 2.",
        "Shape Data Label",
        "Text 2",
        "Text 3.",
        "Text⟦x1⟧ callout",
        "64 VS 32",
    ];
    assert_eq!(texts(&doc), want);
    assert_same_entries(OKAPI, &Vsdx.merge(&doc).unwrap());
    // pseudo translation reads back with the same units and tags
    let mut d = doc.clone();
    pseudo(&mut d);
    let out = Vsdx.merge(&d).unwrap();
    let back = extract(&out);
    assert_eq!(back.units.len(), doc.units.len());
    for (a, b) in doc.units.iter().zip(&back.units) {
        let tags = |u: &Unit| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
        assert_eq!(tags(a), tags(b), "{}", a.id);
    }
    // only the two page parts and pages.xml changed; masters, media and the rest are as they were
    let names = changed_names(OKAPI, &out);
    assert_eq!(names, vec!["visio/pages/pages.xml", "visio/pages/page1.xml", "visio/pages/page2.xml"]);
}

#[test]
fn comments_are_units_only_on_request() {
    assert!(extract(OKAPI).units.iter().all(|u| u.context.style.as_deref() != Some("comment")));
    let mut doc = Vsdx.extract_with_comments(OKAPI, &ExtractOptions::default()).unwrap();
    let cs: Vec<_> = doc.units.iter().filter(|u| u.context.style.as_deref() == Some("comment")).map(|u| (u.id.clone(), plain_text(&u.source()))).collect();
    assert_eq!(cs, vec![("comment-1".to_string(), "My comment".to_string()), ("comment-2".to_string(), "My reply".to_string())]);
    assert_same_entries(OKAPI, &Vsdx.merge(&doc).unwrap());
    set(&mut doc, "comment-2", "Meine Antwort <&>");
    let out = Vsdx.merge(&doc).unwrap();
    let c = part(&out, "visio/comments.xml");
    assert!(c.contains(">Meine Antwort &lt;&amp;&gt;</CommentEntry>") && c.contains(">My comment</CommentEntry>"), "{c}");
    assert!(c.contains("Name='Denis Konovalyenko' Initials='DK'"), "authors untouched");
    assert_eq!(changed_names(OKAPI, &out), vec!["visio/comments.xml"]);
}

#[test]
fn every_language_in_and_out() {
    // each of the 11 languages as the source text and as the target, left to right and right to left
    let shapes: String =
        LANGS.iter().enumerate().map(|(i, (_, t))| format!("<Shape ID='{}' Type='Shape'><Text><cp IX='0'/>{t}\n</Text></Shape>", i + 10)).collect();
    let page = page2().replace("<Shape ID='7' Type='Shape'><Text>Start</Text></Shape>", &shapes);
    let src = Pkg { page2: page, ..Default::default() }.build();
    let doc = extract(&src);
    for (i, (_, t)) in LANGS.iter().enumerate() {
        let u = doc.units.iter().find(|u| u.id == format!("page2-{}", i + 1)).unwrap();
        assert_eq!(plain_text(&u.source()), *t);
    }
    for (code, _) in LANGS {
        let mut d = extract_for(&src, Some(code));
        for (i, (_, t)) in LANGS.iter().enumerate() {
            let target = LANGS[(i + 1) % 11].1;
            let u = d.units.iter().find(|u| u.id == format!("page2-{}", i + 1)).unwrap().clone();
            let _ = t;
            d.set_target(&u.segments[0].id, vec![Inline::text(target)]).unwrap();
        }
        let out = Vsdx.merge(&d).unwrap();
        let p2 = part(&out, "visio/pages/page2.xml");
        for (_, t) in LANGS {
            assert!(p2.contains(t), "{code} {t}");
        }
        let back = extract(&out);
        assert_eq!(back.units.len(), doc.units.len());
        if code == "ar" {
            assert!(p2.contains("<Section N='Paragraph'><Row IX='0'><Cell N='Flags' V='1'/></Row></Section>"), "{p2}");
        } else {
            assert!(!p2.contains("N='Paragraph'"), "{code}");
        }
    }
}

fn with_page2_bytes(page2: Vec<u8>) -> Vec<u8> {
    let mut parts: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), content_types(false).into_bytes()),
        ("visio/pages/pages.xml".into(), pages().into_bytes()),
        ("visio/pages/_rels/pages.xml.rels".into(), pages_rels().into_bytes()),
        ("visio/pages/page1.xml".into(), page1().into_bytes()),
    ];
    parts.push(("visio/pages/page2.xml".into(), page2));
    zip_of(parts)
}

fn utf16(s: &str, be: bool) -> Vec<u8> {
    let mut v = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
    for u in s.encode_utf16() {
        v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
    }
    v
}

#[test]
fn encodings_are_read_and_kept() {
    // UTF-16 with a byte order mark, both orders
    for be in [false, true] {
        let body = page2().replace("encoding='utf-8'", "encoding='utf-16'").replace("Start", "Début 日本");
        let src = with_page2_bytes(utf16(&body, be));
        let mut doc = extract(&src);
        let u = doc.units.iter().find(|u| u.id == "page2-1").unwrap();
        assert_eq!(plain_text(&u.source()), "Début 日本");
        assert_same_entries(&src, &Vsdx.merge(&doc).unwrap());
        set(&mut doc, "page2-1", "Ende ショー");
        let out = Vsdx.merge(&doc).unwrap();
        let raw = members(&out).into_iter().find(|e| e.name == "visio/pages/page2.xml").unwrap().data;
        assert_eq!(&raw[..2], if be { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] });
        assert_eq!(extract(&out).units.iter().find(|u| u.id == "page2-1").map(|u| plain_text(&u.source())).unwrap(), "Ende ショー");
    }
    // UTF-8 with a BOM
    let mut b = vec![0xEF, 0xBB, 0xBF];
    b.extend(page2().replace("Start", "Café").into_bytes());
    let src = with_page2_bytes(b);
    let mut doc = extract(&src);
    assert_same_entries(&src, &Vsdx.merge(&doc).unwrap());
    set(&mut doc, "page2-1", "Fin");
    let raw = members(&Vsdx.merge(&doc).unwrap()).into_iter().find(|e| e.name == "visio/pages/page2.xml").unwrap().data;
    assert_eq!(&raw[..3], &[0xEF, 0xBB, 0xBF]);
    // a declared legacy charset: Shift_JIS and Windows-1252
    for (label, text) in [("shift_jis", "開始ページ"), ("windows-1252", "Début café")] {
        let enc = bilingua_model::encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
        let body = page2().replace("encoding='utf-8'", &format!("encoding='{label}'")).replace("Start", text);
        let (bytes, _, bad) = enc.encode(&body);
        assert!(!bad);
        let src = with_page2_bytes(bytes.into_owned());
        let mut doc = extract(&src);
        let u = doc.units.iter().find(|u| u.id == "page2-1").unwrap();
        assert_eq!(plain_text(&u.source()), text, "{label}");
        assert_same_entries(&src, &Vsdx.merge(&doc).unwrap());
        set(&mut doc, "page2-1", text.chars().rev().collect::<String>().as_str());
        let out = Vsdx.merge(&doc).unwrap();
        let raw = members(&out).into_iter().find(|e| e.name == "visio/pages/page2.xml").unwrap().data;
        let (dec, _, bad) = enc.decode(&raw);
        assert!(!bad && dec.contains(&text.chars().rev().collect::<String>()), "{label} kept");
        assert!(dec.contains(&format!("encoding='{label}'")), "{label} declaration kept");
    }
    // not UTF-8 and no declaration: refused, as the XML specification says
    let body = page2().replace("Start", "Café");
    let (bytes, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.encode(&body);
    refused(&with_page2_bytes(bytes.into_owned()), "not valid XML");
}

fn patched(src: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let mut v = src.to_vec();
    let mut i = 0;
    while i + from.len() <= v.len() {
        if &v[i..i + from.len()] == from {
            v[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
    v
}

#[test]
fn hostile_zip_and_xml() {
    // a member listed twice
    let two = zip_of(vec![("visio/pages/page2.xml".into(), page2().into_bytes()), ("visio/pages/page3.xml".into(), page2().into_bytes())]);
    let dup = patched(&two, b"page3", b"page2");
    // the zip reader keeps one entry per name; bilingua-zip refuses it from the reader's own record count, and the
    // crate's central-directory scan refuses it first, in any letter case
    assert!(bilingua_zip::read_zip("vsdx", &dup, &ZipLimits::default()).is_err());
    refused(&dup, "twice");
    refused(&patched(&two, b"page3", b"PAGE2"), "twice");
    // a path that climbs out of the package
    let esc = zip_of(vec![("visio/pages/page2.xml".into(), page2().into_bytes()), ("visio/pages/AA/p.xml".into(), b"<x/>".to_vec())]);
    refused(&patched(&esc, b"AA", b".."), "climbs");
    let abs = zip_of(vec![("visio/pages/page2.xml".into(), page2().into_bytes()), ("Xvisio/pages/p.xml".into(), b"<x/>".to_vec())]);
    refused(&patched(&abs, b"Xvisio", b"/visio"), "absolute");
    // an entity declaration anywhere in the package (XXE, entity bombs), even in a part nobody reads
    let xxe = "<?xml version='1.0'?><!DOCTYPE m [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><MasterContents>&x;</MasterContents>";
    refused(&Pkg { extra: vec![("visio/masters/master2.xml".into(), xxe.as_bytes().to_vec())], ..Default::default() }.build(), "entit");
    let lol = "<!DOCTYPE l [<!ENTITY a 'aaaaaaaaaa'><!ENTITY b '&a;&a;&a;&a;&a;&a;&a;&a;'>]>";
    refused(&Pkg { page1: page1().replace("<PageContents", &format!("{lol}<PageContents")), ..Default::default() }.build(), "entit");
    // a plain DOCTYPE in a part the filter reads is refused too (LocOS)
    refused(&Pkg { page1: page1().replace("<PageContents", "<!DOCTYPE x><PageContents"), ..Default::default() }.build(), "DOCTYPE");
    // deep nesting
    let deep = format!("<PageContents xmlns='{NS}'>{}{}</PageContents>", "<Shapes>".repeat(400), "</Shapes>".repeat(400));
    match Vsdx.extract(&Pkg { page1: deep, ..Default::default() }.build(), &ExtractOptions::default()) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::XmlDepth),
        r => panic!("{:?}", r.map(|d| d.units.len())),
    }
    // truncated, empty, a zip header alone
    let src = package();
    for cut in [0, 4, 22, src.len() / 2, src.len() - 1] {
        assert!(Vsdx.extract(&src[..cut], &ExtractOptions::default()).is_err(), "cut {cut}");
    }
    // not well formed in several ways
    for bad in ["<PageContents><Shapes></PageContents>", "<PageContents>&nbsp;</PageContents>", "<PageContents x='1' x='2'/>", "<a/><b/>", "<PageContents>\u{1}</PageContents>", ""] {
        assert!(Vsdx.extract(&Pkg { page1: bad.into(), ..Default::default() }.build(), &ExtractOptions::default()).is_err(), "{bad:?}");
    }
    // a path in the relationships that leaves the package finds no page
    let rels = pages_rels().replace("page1.xml", "../../page1.xml");
    let doc = Vsdx
        .extract(&zip_of(vec![("visio/pages/pages.xml".into(), pages().into_bytes()), ("visio/pages/_rels/pages.xml.rels".into(), rels.into_bytes()), ("visio/pages/page2.xml".into(), page2().into_bytes())]), &ExtractOptions::default())
        .unwrap();
    assert_eq!(doc.units.len(), 3);
    // a damaged skeleton is an error, not a broken file
    let mut d = extract(&src);
    d.skeleton = Skeleton::default();
    assert!(Vsdx.merge(&d).is_err());
    let mut d = extract(&src);
    if let Skeleton::Zip { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-docx", "0", 1));
    }
    assert!(matches!(Vsdx.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn text_edge_cases() {
    // entities, CDATA, a comment inside the text, tabs, character references, windows line ends
    let page = page2().replace(
        "<Text>Start</Text>",
        "<Text>  <![CDATA[a < b]]> &amp; &#x41;&#10;\tb  </Text></Shape><Shape ID='8'><Text><!-- c -->Tail<!-- d --> more\r\nline</Text></Shape><Shape ID='9'><Text>   </Text></Shape><Shape ID='11'><Text/></Shape><Shape ID='12'><Text>\u{a0}x\u{a0}</Text>",
    );
    let src = Pkg { page2: page, ..Default::default() }.build();
    let mut doc = extract(&src);
    let t: Vec<String> = doc.units.iter().filter(|u| u.id.starts_with("page2")).map(|u| plain_text(&u.source())).collect();
    assert_eq!(t, vec!["a < b & A\n\tb", "Tail more\nline", "x"]);
    assert_same_entries(&src, &Vsdx.merge(&doc).unwrap());
    set(&mut doc, "page2-1", "x < y");
    set(&mut doc, "page2-2", "A\nB");
    let out = part(&Vsdx.merge(&doc).unwrap(), "visio/pages/page2.xml");
    assert!(out.contains("<Text>  x &lt; y  </Text>"), "{out}");
    assert!(out.contains("<Text><!-- c -->A\r\nB</Text>"), "{out}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        let _ = Vsdx.extract(&bytes, &ExtractOptions::default());
        let mut zipish = b"PK\x03\x04".to_vec();
        zipish.extend(&bytes);
        let _ = Vsdx.extract(&zipish, &ExtractOptions::default());
    }

    #[test]
    fn mutated_pages_never_panic(edits in proptest::collection::vec((0usize..4000, 0u8..4, any::<u8>()), 1..6)) {
        let mut b = page1().into_bytes();
        for (pos, op, byte) in edits {
            let p = pos % b.len().max(1);
            match op {
                0 => b[p] = byte,
                1 => { b.remove(p); }
                2 => b.insert(p, byte),
                _ => { let end = (p + 20).min(b.len()); b.drain(p..end); }
            }
        }
        let src = Pkg { page1: String::from_utf8_lossy(&b).into_owned(), ..Default::default() }.build();
        if let Ok(mut doc) = Vsdx.extract(&src, &ExtractOptions::new().with_target_lang("ar")) {
            // whatever was read can be pseudo translated, written and read again
            pseudo(&mut doc);
            let out = Vsdx.merge(&doc).expect("a document this filter extracted merges");
            let again = Vsdx.extract(&out, &ExtractOptions::default()).expect("what it wrote reads again");
            prop_assert_eq!(again.units.len(), doc.units.len());
        }
    }

    #[test]
    fn random_text_round_trips(t in "[^\u{0}-\u{8}\u{b}\u{c}\u{e}-\u{1f}\u{fffe}\u{ffff}]{1,60}") {
        // Python's letter test, as the extractor uses (Rust's is_alphanumeric also takes some combining marks)
        prop_assume!(bilingua_model::py_has_alnum(&t));
        let src = package();
        let mut doc = extract(&src);
        doc.set_target("page2-1", vec![Inline::text(t.clone())]).unwrap();
        let out = Vsdx.merge(&doc).unwrap();
        let again = extract(&out);
        let got = plain_text(&again.units.iter().find(|u| u.id == "page2-1").unwrap().source());
        let want = t.clone();
        prop_assert_eq!(got.trim_matches(crate::xml::py_space), want.trim_matches(crate::xml::py_space));
    }
}

#[test]
fn libreoffice_opens_the_merged_file_when_installed() {
    let candidates = [
        "C:\\Program Files\\LibreOffice\\program\\soffice.exe",
        "C:\\Program Files (x86)\\LibreOffice\\program\\soffice.exe",
        "/usr/bin/soffice",
        "/usr/local/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ];
    let Some(soffice) = candidates.iter().find(|p| std::path::Path::new(p).exists()) else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bilingua-vsdx-lo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let convert = |name: &str, bytes: &[u8]| -> bool {
        let file = dir.join(name);
        std::fs::write(&file, bytes).unwrap();
        let profile = format!("-env:UserInstallation=file:///{}", dir.join("profile").to_string_lossy().replace('\\', "/"));
        let _ = std::process::Command::new(soffice).arg(&profile).args(["--headless", "--convert-to", "pdf", "--outdir"]).arg(&dir).arg(&file).output().unwrap();
        dir.join(name.replace(".vsdx", ".pdf")).exists()
    };
    // LibreOffice imports Visio read only; if it cannot open the original, there is nothing to compare
    let orig_ok = convert("original.vsdx", OKAPI);
    let mut doc = extract(OKAPI);
    pseudo(&mut doc);
    let out = Vsdx.merge(&doc).unwrap();
    let merged_ok = convert("merged.vsdx", &out);
    let _ = std::fs::remove_dir_all(&dir);
    if !orig_ok {
        eprintln!("skipped: this LibreOffice cannot convert the original Visio file");
        return;
    }
    assert!(merged_ok, "LibreOffice opens the original but not the translated file");
}


/// Writes the oracle corpus: `VSDX_CORPUS_DIR=... cargo test -p bilingua-vsdx -- --ignored write_corpus`.
#[test]
#[ignore]
fn write_corpus() {
    let dir = std::env::var("VSDX_CORPUS_DIR").expect("set VSDX_CORPUS_DIR");
    let w = |name: &str, b: &[u8]| std::fs::write(std::path::Path::new(&dir).join(name), b).unwrap();
    w("fixture.vsdx", &package());
    w("fixture.vsdm", &Pkg { vba: true, ..Default::default() }.build());
    w("okapi-2-pages.vsdx", OKAPI);
    let shapes: String = LANGS.iter().enumerate().map(|(i, (_, t))| format!("<Shape ID='{}' Type='Shape'><Text><cp IX='0'/>{t}\n</Text></Shape>", i + 10)).collect();
    let page = page2().replace("<Shape ID='7' Type='Shape'><Text>Start</Text></Shape>", &shapes);
    w("languages.vsdx", &Pkg { page2: page, ..Default::default() }.build());
    let body = page2().replace("encoding='utf-8'", "encoding='utf-16'").replace("Start", "Début 日本");
    w("utf16.vsdx", &with_page2_bytes(utf16(&body, false)));
    let sj = page2().replace("encoding='utf-8'", "encoding='shift_jis'").replace("Start", "開始ページ");
    let (b, _, _) = bilingua_model::encoding_rs::SHIFT_JIS.encode(&sj);
    w("shiftjis.vsdx", &with_page2_bytes(b.into_owned()));
    w("rtl-paragraph.vsdx", &Pkg { page1: super::tests::rtl_page1_pub(), ..Default::default() }.build());
}
