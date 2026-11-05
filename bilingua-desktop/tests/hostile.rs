//! Hostile input: malformed, truncated, deep and oversized files give typed errors and never panic; property
//! tests over random bytes, byte-mutated fixtures and generated documents.

mod common;
use bilingua_desktop::*;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn both(bytes: &[u8]) -> [Result<Document, Error>; 2] {
    [desktop(bytes), catkeys(bytes)]
}

#[test]
fn malformed_files_are_typed_errors() {
    let cases: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"   ".to_vec(),
        b"\n\n\n".to_vec(),
        b"[".to_vec(),
        b"[Desktop Entry".to_vec(),
        b"]".to_vec(),
        b"=".to_vec(),
        b"\t\t\t".to_vec(),
        b"1\t".to_vec(),
        b"1\t\t\t".to_vec(),
        b"\xef\xbb\xbf".to_vec(),
        b"\xff\xfe[\x00D\x00".to_vec(), // UTF-16 LE
        b"\xfe\xff\x00[".to_vec(),      // UTF-16 BE
        b"\x00\x01\x02\x03".to_vec(),
        b"[Desktop Entry]\nName=\xc3(\n".to_vec(), // invalid UTF-8
        b"1\tenglish\tx\t1\nA\xe9\t\t\t\n".to_vec(),
        b"1\tenglish\tx\t99999999999999999999999\nA\t\t\tB\n".to_vec(),
        b"1\tenglish\tx\t1\n\t\t\n".to_vec(),
        b"1\tenglish\tx\t1\n \n".to_vec(),
    ];
    for c in &cases {
        for r in both(c) {
            if let Err(e) = r {
                assert!(is_typed(&e), "{c:?}: {e}");
            }
        }
    }
    // each of these is refused by the filter whose format it is not
    assert!(desktop(b"").is_err() && catkeys(b"").is_err());
    assert!(desktop(b"[Desktop Entry").is_err());
    assert!(catkeys(b"1\tenglish\tx\t1\n \n").is_err());
    assert!(matches!(desktop(b"\xff\xfe[\x00D\x00"), Err(Error::Encoding(_))));
    assert!(matches!(catkeys(b"\xfe\xff\x00["), Err(Error::Encoding(_))));
    assert!(matches!(desktop(b"[Desktop Entry]\nName=\xc3(\n"), Err(Error::Encoding(_))));
}

#[test]
fn truncated_fixtures_never_panic() {
    for (name, bytes) in fixtures() {
        for cut in 0..bytes.len() {
            if let Ok(d) = extract_any(&name, &bytes[..cut]) {
                // whatever extracts must merge back to the same bytes
                assert_eq!(merge(&d), &bytes[..cut], "{name} cut at {cut}");
            }
        }
    }
}

#[test]
fn deep_and_huge_files() {
    // very many groups, very long lines, very many keys
    let mut deep = String::from("[Desktop Entry]\n");
    for i in 0..20_000 {
        deep.push_str(&format!("[Group {i}]\nName[x]=y\n"));
    }
    let d = desktop(deep.as_bytes()).unwrap();
    assert!(d.units.is_empty());
    let long = format!("[Desktop Entry]\nName={}\n", "a".repeat(2_000_000));
    assert!(matches!(desktop(long.as_bytes()), Err(Error::Limit(l)) if l.kind == LimitKind::UnitChars));
    let nested = format!("[Desktop Entry]\nKeywords={}\n", "\\".repeat(500_000));
    let _ = desktop(nested.as_bytes());
    let semis = format!("[Desktop Entry]\nKeywords={}\n", ";a".repeat(60_000));
    assert!(matches!(desktop(semis.as_bytes()), Err(Error::Limit(l)) if l.kind == LimitKind::Units));
    let ck = format!("1\tenglish\tx\t1\n{}", "S\t\t\t\n".repeat(60_000));
    assert!(matches!(catkeys(ck.as_bytes()), Err(Error::Limit(l)) if l.kind == LimitKind::Units));
    let nl = format!("1\tenglish\tx\t1\n{}", "\n".repeat(1_100_000));
    assert!(matches!(catkeys(nl.as_bytes()), Err(Error::Limit(l)) if l.kind == LimitKind::Lines));
    // the input size limit
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..Default::default() };
    let big = vec![b'a'; 1000];
    assert!(matches!(DesktopFilter.extract(&big, &o), Err(Error::Limit(l)) if l.kind == LimitKind::InputBytes));
    assert!(matches!(CatkeysFilter.extract(&big, &o), Err(Error::Limit(l)) if l.kind == LimitKind::InputBytes));
    // a catalog of 100 000 tab runs
    let tabs = format!("1\tenglish\tx\t1\n{}\n", "\t".repeat(100_000));
    let r = catkeys(tabs.as_bytes());
    assert!(r.is_ok() || r.is_err());
}

#[test]
fn lone_carriage_returns_and_odd_endings_round_trip() {
    for data in [
        &b"[Desktop Entry]\rName=x\r"[..],
        b"[Desktop Entry]\r\nName=x\r",
        b"[Desktop Entry]\nName=x\r",
        b"[Desktop Entry]\n\r\nName=x\n\r\n",
        b"[Desktop Entry]\nName=x\n\n\n",
    ] {
        if let Ok(d) = desktop(data) {
            assert_eq!(merge(&d), data);
            let mut d2 = d.clone();
            d2.target_lang = Some("de".into());
            pseudo(&mut d2);
            let out = merge(&d2);
            assert_eq!(desktop(&out).unwrap().units.len(), d.units.len());
        }
    }
    let ck = b"1\tenglish\tx\t1\rA\t\t\tB\r";
    if let Ok(d) = catkeys(ck) {
        assert_eq!(merge(&d), ck);
    }
}

fn exercise(name: &str, bytes: &[u8]) {
    let Ok(d) = extract_any(name, bytes) else { return };
    assert_eq!(merge(&d), bytes);
    let n = d.units.len();
    let src = texts(&d);
    for target in [None, Some("de-DE"), Some("zh-Hans"), Some("ar")] {
        let mut t = d.clone();
        t.target_lang = target.map(str::to_string);
        pseudo(&mut t);
        let out = merge(&t);
        let back = extract_any(name, &out).unwrap_or_else(|e| panic!("{name} re-extract ({target:?}) failed: {e}\n{:?}", String::from_utf8_lossy(bytes)));
        assert_eq!(back.units.len(), n);
        if target.is_some() || name.ends_with(".catkeys") {
            assert_eq!(texts(&back), src);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2000)) {
        for r in both(&bytes) {
            if let Err(e) = &r { prop_assert!(is_typed(e)); }
        }
        exercise("x.desktop", &bytes);
        exercise("x.catkeys", &bytes);
    }

    #[test]
    fn random_text_never_panics(s in "[\\[\\]A-Za-z0-9=;\\\\ \t\r\n#@_.-]{0,600}") {
        exercise("x.desktop", s.as_bytes());
        let ck = format!("1\ten\tx\t1\n{s}");
        exercise("x.catkeys", ck.as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..64, ops in proptest::collection::vec((0usize..4096, any::<u8>(), 0u8..4), 1..8)) {
        let all = fixtures();
        let (name, orig) = &all[idx % all.len()];
        let mut b = orig.clone();
        for (pos, val, kind) in ops {
            if b.is_empty() { break; }
            let p = pos % b.len();
            match kind {
                0 => b[p] = val,
                1 => { b.remove(p); }
                2 => b.insert(p, val),
                _ => b.truncate(p + 1),
            }
        }
        exercise(name, &b);
    }

    #[test]
    fn generated_desktop_documents_round_trip(lines in proptest::collection::vec(desktop_line(), 0..30), crlf in any::<bool>(), eof in any::<bool>()) {
        let nl = if crlf { "\r\n" } else { "\n" };
        let mut s = String::from("[Desktop Entry]");
        s.push_str(nl);
        s.push_str(&lines.join(nl));
        if eof { s.push_str(nl); }
        exercise("g.desktop", s.as_bytes());
    }

    #[test]
    fn generated_catkeys_documents_round_trip(lines in proptest::collection::vec(catkeys_line(), 0..30), crlf in any::<bool>(), eof in any::<bool>()) {
        let nl = if crlf { "\r\n" } else { "\n" };
        let mut s = String::from("1\tenglish\tx-vnd.G\t77");
        s.push_str(nl);
        s.push_str(&lines.join(nl));
        if eof { s.push_str(nl); }
        exercise("g.catkeys", s.as_bytes());
    }
}

fn desktop_line() -> impl Strategy<Value = String> {
    let val = "[A-Za-z0-9 ;\\\\\u{e9}\u{65e5}\t]{0,20}";
    prop_oneof![
        (prop::sample::select(vec!["Name", "GenericName", "Comment", "Keywords", "Exec"]), val).prop_map(|(k, v)| format!("{k}={v}")),
        (prop::sample::select(vec!["Name", "Comment", "Keywords"]), prop::sample::select(vec!["de", "fr_FR", "zh_CN", "sr@latin"]), val)
            .prop_map(|(k, l, v)| format!("{k}[{l}]={v}")),
        prop::sample::select(vec!["[Desktop Action a]", "[X-Foo]", "[Desktop Entry]", "# note", "", "junk", "Name"]).prop_map(str::to_string),
    ]
}

fn catkeys_line() -> impl Strategy<Value = String> {
    let f = "[A-Za-z0-9 %\\\\\"\u{e9}\u{65e5}]{0,12}";
    (f, f, f, f).prop_map(|(a, b, c, d)| format!("{a}\t{b}\t{c}\t{d}"))
}
