//! Hostile input: malformed, truncated, deep and huge files give typed errors (or a clean parse), and a property test over
//! random bytes and byte-mutated fixtures never panics, never loses a unit and never hangs.
mod common;
use bilingua_htmlhelp::*;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;
use std::time::Instant;

fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(HhcFilter), Box::new(HhkFilter), Box::new(HhpFilter)]
}

fn sitemap_entry(name: &str) -> Vec<u8> {
    format!("<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"{name}\"></OBJECT>\n").into_bytes()
}

#[test]
fn malformed_and_truncated_files_give_typed_errors() {
    let cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0xEF, 0xBB, 0xBF],
        vec![0xEF, 0xBB, 0xBF, 0xFF, 0xFE],
        vec![0xFF, 0xFE],
        vec![0xFF, 0xFE, 0x41],
        vec![0xFF, 0xFE, 0x00, 0xD8],
        vec![0xFE, 0xFF, 0xDC, 0x00],
        b"\x00\x00\x00\x00".to_vec(),
        b"<".to_vec(),
        b"<!--".to_vec(),
        b"<OBJECT".to_vec(),
        b"<OBJECT type=\"text/sitemap\"".to_vec(),
        b"<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"".to_vec(),
        b"[".to_vec(),
        b"[OPTIONS".to_vec(),
        b"[OPTIONS]\r\nTitle".to_vec(),
        b"[WINDOWS]\r\nmain=\"".to_vec(),
        b"\xff\xff\xff\xff\xff".to_vec(),
        b"\x81\x82\x83".to_vec(),
    ];
    for data in &cases {
        for f in filters() {
            match f.extract(data, &ExtractOptions::default()) {
                Ok(doc) => {
                    let _ = f.merge(&doc);
                }
                Err(e) => assert!(matches!(e, Error::Parse { .. } | Error::Encoding(_) | Error::Limit(_)), "{e:?}"),
            }
        }
    }
    // truncated copies of a real file at every length: parse or a typed error, never a panic
    let full = cp1252(HHC);
    for n in 0..full.len() {
        let _ = HhcFilter.extract(&full[..n], &ExtractOptions::default());
    }
    let hp = cp1252(HHP);
    for n in 0..hp.len() {
        let _ = HhpFilter.extract(&hp[..n], &ExtractOptions::default());
    }
}

#[test]
fn deep_and_huge_files() {
    let t0 = Instant::now();
    // five megabytes of '<' (over the pre-parse budget), and of '<a ' (under the budget, never a tag)
    let e = HhcFilter.extract(&vec![b'<'; 5_000_000], &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Limit(_)), "{e}");
    let junk = b"<a ".repeat(600_000);
    assert!(HhcFilter.extract(&junk, &ExtractOptions::default()).is_err());
    // nesting without end: ten thousand open objects with a name each
    let mut deep = Vec::new();
    for i in 0..10_000 {
        deep.extend_from_slice(format!("<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"n{i}\">").as_bytes());
    }
    assert_eq!(extract(&HhcFilter, &deep).unwrap().units.len(), 10_000);
    // a very long attribute name, value and line
    let long = [b"<OBJECT type=\"text/sitemap\"><param ".to_vec(), vec![b'x'; 3_000_000], b"=1 name=Name value=ok>".to_vec()].concat();
    let _ = HhcFilter.extract(&long, &ExtractOptions::default());
    let big_value = [b"<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"".to_vec(), vec![b'a'; 100_001], b"\">".to_vec()].concat();
    assert!(matches!(HhcFilter.extract(&big_value, &ExtractOptions::default()).unwrap_err(), Error::Limit(_)));
    // more lines than the line budget
    let lines = vec![b'\n'; 1_000_001];
    assert!(matches!(HhpFilter.extract(&lines, &ExtractOptions::default()).unwrap_err(), Error::Limit(_)));
    // more units than allowed
    let many: Vec<u8> = (0..50_001).flat_map(|i| sitemap_entry(&format!("e{i}"))).collect();
    assert!(matches!(HhcFilter.extract(&many, &ExtractOptions::default()).unwrap_err(), Error::Limit(_)));
    // over the input size limit
    let tiny = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..Default::default() };
    assert!(matches!(HhcFilter.extract(&cp1252(HHC), &tiny).unwrap_err(), Error::Limit(_)));
    // an hhp of ten thousand sections and windows
    let mut hp = String::from("[OPTIONS]\r\nTitle=T\r\n[WINDOWS]\r\n");
    for i in 0..20_000 {
        hp.push_str(&format!("[S{i}]\r\nw{i}=\"cap {i}\",x\r\n"));
    }
    assert_eq!(HhpFilter.extract(hp.as_bytes(), &ExtractOptions::default()).unwrap().units.len(), 1);
    assert!(t0.elapsed().as_secs() < 30, "{:?}", t0.elapsed());
}

#[test]
fn a_skeleton_that_does_not_match_its_file_is_refused() {
    let data = cp1252(HHC);
    let mut doc = extract(&HhcFilter, &data).unwrap();
    doc.units.pop();
    assert!(HhcFilter.merge(&doc).is_err());
    let mut doc = extract(&HhcFilter, &data).unwrap();
    doc.format = "hhc".into();
    if let Skeleton::Text { parts, .. } = &mut doc.skeleton {
        parts.push(SkelPart::Unit(99));
    }
    assert!(HhcFilter.merge(&doc).is_err());
    // another crate's skeleton
    let po = extract(&HhcFilter, &data).unwrap();
    let mut other = po.clone();
    if let Skeleton::Text { parser, .. } = &mut other.skeleton {
        *parser = Some(ParserInfo::new("bilingua-po", "0.1.0", 1));
    }
    assert!(matches!(HhcFilter.merge(&other), Err(Error::ParserVersion { .. })));
}

#[test]
fn translations_cannot_break_the_markup() {
    let data = cp1252(HHC);
    let nasty = ["\"><script>alert(1)</script>", "'; DROP", "a\r\nb", "</OBJECT><OBJECT type=text/sitemap>", "<!--", "-->", "&amp;&lt;", "\u{1}\u{7f}","\u{feff}x"];
    for t in nasty {
        // a value with no letter or digit is not a unit any more (as in Localization OS), so each one gets a letter
        let t = &format!("T{t}");
        let out = build(&HhcFilter, &data, &[(0, t), (1, t), (2, t), (3, t)]);
        let doc = extract(&HhcFilter, &out).unwrap_or_else(|e| panic!("{t:?}: {e}
{}", String::from_utf8_lossy(&out)));
        assert_eq!(doc.units.len(), 4, "{t:?}");
        assert!(!contains(&out, b"<script>"));
    }
    // a NUL would make the file read as binary: refused, not written
    assert!(merge_with(&HhcFilter, &data, &[(0, "T\0")], None, MergeOptions::default()).is_err());
    // a value a browser does not read as an attribute value (here: inside an unclosed <title>) is written opaque;
    // one after a closed <title> is an ordinary value (as Localization OS, which asks a browser parse which is which)
    let closed = b"<html><title>x</title><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"a b\"></OBJECT>";
    let out = build(&HhcFilter, closed, &[(0, "c d=e/f")]);
    assert!(contains(&out, b"value=\"c d=e/f\""), "{}", String::from_utf8_lossy(&out));
    let risky = b"<html><title>x<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"a b\"></OBJECT>";
    let out = build(&HhcFilter, risky, &[(0, "c d=e/f")]);
    assert!(contains(&out, b"value=\"c&#32;d&#61;e&#47;f\""), "{}", String::from_utf8_lossy(&out));
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v = vec![cp1252(HHC), cp1252(HHK), cp1252(HHP), HHC.as_bytes().to_vec()];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus");
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut names: Vec<_> = rd.flatten().map(|f| f.path()).collect();
        names.sort();
        v.extend(names.iter().step_by(3).map(|p| std::fs::read(p).unwrap()));
    }
    v
}

fn exercise(f: &dyn Filter, data: &[u8]) {
    let Ok(mut doc) = f.extract(data, &ExtractOptions::default()) else { return };
    let n = doc.units.len();
    assert!(n > 0);
    let unit_ids: Vec<String> = doc.units.iter().map(|u| u.id.clone()).collect();
    let untouched = f.merge(&doc).expect("untouched merge");
    if std::str::from_utf8(data).is_ok() || data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]) {
        assert_eq!(untouched, data, "untouched merge must give back the bytes");
    }
    for (i, id) in unit_ids.iter().enumerate() {
        doc.set_target(id, vec![Inline::text(format!("T{i} \"x\" <y> & z"))]).expect("set_target");
    }
    for target in [None, Some("de-DE"), Some("ja-JP"), Some("ru"), Some("vi"), Some("hi-IN"), Some("zh-Hant")] {
        doc.target_lang = target.map(str::to_string);
        match f.merge(&doc) {
            Ok(out) => {
                let back = f.extract(&out, &ExtractOptions::default());
                if let Ok(b) = back {
                    assert_eq!(b.units.len(), n, "unit count after a merge for {target:?}");
                }
            }
            Err(e) => assert!(matches!(e, Error::Write(_)), "{e:?}"),
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..1500)) {
        for f in filters() {
            let _ = f.extract(&data, &ExtractOptions::default());
        }
    }

    #[test]
    fn random_markup_never_panics(data in "[<>/=\"' a-zA-Z!\\-\\[\\]&#;\\r\\n\\t]{0,600}") {
        for f in filters() {
            exercise(f.as_ref(), data.as_bytes());
        }
    }

    #[test]
    fn mutated_fixtures_keep_every_unit(pick in 0usize..100, edits in proptest::collection::vec((any::<u16>(), 0u8..4, any::<u8>(), 1usize..12), 1..6)) {
        let fx = fixtures();
        let mut data = fx[pick % fx.len()].clone();
        for (pos, op, byte, len) in edits {
            if data.is_empty() { break; }
            let p = pos as usize % data.len();
            match op {
                0 => data[p] = byte,
                1 => { data.drain(p..(p + len).min(data.len())); }
                2 => { let chunk: Vec<u8> = data[p..(p + len).min(data.len())].to_vec(); data.splice(p..p, chunk); }
                _ => data.insert(p, b"<>\"'=&;/[]\r\n"[byte as usize % 12]),
            }
        }
        for f in filters() {
            exercise(f.as_ref(), &data);
        }
    }
}
