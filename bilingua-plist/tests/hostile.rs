//! Malformed, truncated, deep, shared and huge input: typed errors, never a panic. Whatever parses merges back to
//! its own bytes, and a pseudo-translated merge of it parses again.
mod common;
use bilingua_model::*;
use bilingua_plist::{plist_records, stringsdict_records, Limits, Plist, Stringsdict};
use common::*;
use proptest::prelude::*;
use std::path::Path;

fn corpus_file(name: &str) -> Vec<u8> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let a = root.join("../oracle/corpus/plist").join(name);
    std::fs::read(a).unwrap()
}

fn fixtures() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = vec![
        include_bytes!("data/Info.plist").to_vec(),
        include_bytes!("data/Root.plist").to_vec(),
        include_bytes!("data/Localizable.stringsdict").to_vec(),
    ];
    for n in [
        "binary_info.plist",
        "binary_ru.stringsdict",
        "edge.stringsdict",
        "edge_generic.plist",
        "edge_usage.plist",
        "lang_ja_utf16le.plist",
        "lang_ar_utf16be.plist",
        "lang_de_utf8bom_crlf.plist",
        "lang_ar.stringsdict",
        "settings.plist",
    ] {
        v.push(corpus_file(n));
    }
    v
}

/// Whichever way a file is opened: no panic; an Ok document merges to the same bytes; a pseudo-translated merge
/// either is refused with a write error or parses again with the same units.
fn check(b: &[u8]) {
    let _ = Plist.sniff(b);
    let _ = Stringsdict.sniff(b);
    let _ = plist_records(b);
    let _ = stringsdict_records(b);
    for f in [F::Plist, F::Sd] {
        let opts = ExtractOptions { include_locked: true, target_lang: Some("ru".into()), ..Default::default() };
        let Ok(d) = f.filter().extract(b, &opts) else { continue };
        assert_eq!(merged(f, &d), b, "a parsed file must merge back unchanged");
        let mut p = d.clone();
        for u in d.units.iter().filter(|u| u.translate) {
            let mut t = vec![Inline::text("<&>]]>\u{1}\u{b}\"")];
            t.extend(u.source());
            let _ = p.set_target(&u.segments[0].id, t);
        }
        let out = match f.filter().merge(&p) {
            Ok(o) => o,
            Err(Error::Write(_)) => continue,
            Err(e) => panic!("{e}"),
        };
        let back = f.filter().extract(&out, &ExtractOptions { include_locked: true, ..Default::default() }).expect("a merged file must parse again");
        assert!(back.units.len() >= d.units.len());
    }
}

#[test]
fn every_truncation_of_every_fixture() {
    for f in fixtures() {
        for n in 0..=f.len() {
            check(&f[..n]);
        }
    }
}

#[test]
fn every_byte_of_the_binary_fixtures_mutated() {
    for name in ["binary_info.plist", "binary_ru.stringsdict"] {
        let f = corpus_file(name);
        for i in 0..f.len() {
            for v in [0x00u8, 0x01, 0x7F, 0x80, 0xFF, f[i].wrapping_add(1), f[i] ^ 0x10] {
                let mut b = f.clone();
                b[i] = v;
                check(&b);
            }
        }
    }
}

fn deep_binary(depth: usize) -> Vec<u8> {
    // a chain of one-element arrays, each pointing at the next object
    let mut out = b"bplist00".to_vec();
    let mut offs = Vec::new();
    for i in 0..depth {
        offs.push(out.len() as u32);
        out.push(0xA1);
        out.extend_from_slice(&((i + 1) as u16).to_be_bytes());
    }
    offs.push(out.len() as u32);
    out.extend_from_slice(&[0x52, b'h', b'i']);
    let table = out.len() as u64;
    for o in &offs {
        out.extend_from_slice(&o.to_be_bytes());
    }
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&[4, 2]);
    out.extend_from_slice(&((depth + 1) as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&table.to_be_bytes());
    out
}

/// Objects `0..levels` are arrays of `fan` references to the next object: a tree of `fan^levels` leaves from a few
/// hundred bytes.
fn shared_binary(levels: usize, fan: usize) -> Vec<u8> {
    let mut out = b"bplist00".to_vec();
    let mut offs = Vec::new();
    for i in 0..levels {
        offs.push(out.len() as u16);
        out.push(0xAF);
        out.extend_from_slice(&[0x10, fan as u8]);
        for _ in 0..fan {
            out.push((i + 1) as u8);
        }
    }
    offs.push(out.len() as u16);
    out.extend_from_slice(&[0x52, b'h', b'i']);
    let table = out.len() as u64;
    for o in &offs {
        out.extend_from_slice(&o.to_be_bytes());
    }
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&[2, 1]);
    out.extend_from_slice(&((levels + 1) as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&table.to_be_bytes());
    out
}

fn limit_kind(e: &Error) -> Option<LimitKind> {
    if let Error::Limit(l) = e {
        Some(l.kind)
    } else {
        None
    }
}

#[test]
fn deep_shared_and_huge() {
    // a chain of arrays 100 deep is fine; 129 deep is not
    assert!(Plist.extract(&deep_binary(100), &ExtractOptions::default()).is_ok());
    assert_eq!(limit_kind(&parse_err(F::Plist, &deep_binary(129))), Some(LimitKind::XmlDepth));
    assert_eq!(limit_kind(&parse_err(F::Plist, &deep_binary(20_000))), Some(LimitKind::XmlDepth));
    // a shared-object bomb: 100^40 leaves from 4 KB
    let bomb = shared_binary(40, 100);
    let t = std::time::Instant::now();
    assert_eq!(limit_kind(&parse_err(F::Plist, &bomb)), Some(LimitKind::Units));
    assert!(t.elapsed().as_secs() < 5, "refused slowly: {:?}", t.elapsed());
    // XML nesting bomb of 100 000 levels
    let deep = format!("<plist>{}<string>x</string>{}</plist>", "<array>".repeat(100_000), "</array>".repeat(100_000));
    assert!(limit_kind(&parse_err(F::Plist, deep.as_bytes())).is_some());
    // a very long value, key, comment and run of entities, all allowed
    let big = format!(
        "<!-- {} --><plist><dict><key>{}</key><string>{}</string><key>CFBundleName</key><string>{}</string></dict></plist>",
        "c".repeat(3_000_000),
        "k".repeat(1_000_000),
        "v".repeat(90_000),
        "&amp;".repeat(20_000)
    );
    let d = ex(F::Plist, big.as_bytes());
    assert_eq!(merged(F::Plist, &d), big.as_bytes());
    // a value over the character limit
    let over = format!("<plist><string>{}</string></plist>", "v".repeat(100_001));
    assert_eq!(limit_kind(&parse_err(F::Plist, over.as_bytes())), Some(LimitKind::UnitChars));
    // over the byte cap
    let small = Limits { max_input_bytes: 100, ..Limits::default() };
    assert_eq!(limit_kind(&Plist.extract_with(&[b' '; 101], &ExtractOptions::default(), &small).unwrap_err()), Some(LimitKind::InputBytes));
    // a newline bomb
    let nl = format!("<plist><string>x</string></plist>{}", "\n".repeat(20 * 50_000 + 1));
    assert_eq!(limit_kind(&parse_err(F::Plist, nl.as_bytes())), Some(LimitKind::Lines));
    // too many entries, as strings and as bare elements
    let many = format!("<plist><array>{}</array></plist>", "<string>Hello there</string>".repeat(50_001));
    assert_eq!(limit_kind(&parse_err(F::Plist, many.as_bytes())), Some(LimitKind::Units));
    let flags = format!("<plist><array>{}</array></plist>", "<true/>".repeat(2_000_001));
    assert_eq!(limit_kind(&parse_err(F::Plist, flags.as_bytes())), Some(LimitKind::Units));
    // deep nesting under long keys: the path text that would be built is capped
    let mut keys = String::new();
    for _ in 0..100 {
        keys.push_str(&format!("<dict><key>{}</key>", "k".repeat(50_000)));
    }
    keys.push_str("<array>");
    for _ in 0..20_000 {
        keys.push_str("<string>Hello there</string>");
    }
    keys.push_str("</array>");
    keys.push_str(&"</dict>".repeat(100));
    let t = std::time::Instant::now();
    assert!(Plist.extract(format!("<plist>{keys}</plist>").as_bytes(), &ExtractOptions::default()).is_err());
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
}

#[test]
fn entity_bombs_and_external_entities_are_refused_in_every_encoding() {
    let laughs = "<?xml version=\"1.0\"?>\n<!DOCTYPE plist [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\"><!ENTITY c \"&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;\">]>\n<plist><string>&c;</string></plist>";
    let xxe = "<?xml version=\"1.0\"?>\n<!DOCTYPE plist [<!ENTITY x SYSTEM \"file:///etc/passwd\">]>\n<plist><string>&x;</string></plist>";
    let parameter = "<!DOCTYPE plist [<!ENTITY % p SYSTEM \"http://example.invalid/e.dtd\"> %p;]><plist><string>x</string></plist>";
    let subset_only = "<!DOCTYPE plist [<!ELEMENT plist ANY>]><plist><string>Hello there</string></plist>";
    for src in [laughs, xxe, parameter, subset_only] {
        let utf16le: Vec<u8> = [0xFF, 0xFE].iter().copied().chain(src.encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
        let utf16be: Vec<u8> = [0xFE, 0xFF].iter().copied().chain(src.encode_utf16().flat_map(|u| u.to_be_bytes())).collect();
        for bytes in [src.as_bytes().to_vec(), utf16le, utf16be] {
            let e = parse_err(F::Plist, &bytes);
            assert!(matches!(e, Error::Limit(_) | Error::Parse { .. }), "{e:?}");
            if src != subset_only {
                assert_eq!(limit_kind(&e), Some(LimitKind::XmlEntities), "{e:?}");
            }
        }
    }
    // the five predefined entities and character references are all a plist needs
    let ok = "<plist><string>&lt;&gt;&amp;&quot;&apos;&#65;&#x42;</string></plist>";
    assert_eq!(units_of(F::Plist, ok.as_bytes()), ["<>&\"'AB"]);
}

#[test]
fn bad_encodings_and_binary_headers_are_typed_errors() {
    for b in [
        &b"\xff\xfe\x00"[..],                          // odd UTF-16
        &b"\xff\xfe\x00\xd8x\x00"[..],                 // lone high surrogate
        &b"\xff\xfe\x00\x00"[..],                      // UTF-32 LE BOM
        &b"\x00\x00\xfe\xff"[..],                      // UTF-32 BE BOM
        &b"\xef\xbb\xbf\xff\xfe"[..],                  // invalid UTF-8 after the BOM
        &b"<plist>\x00</plist>"[..],                   // NUL
        &b"<plist><string>\x01</string></plist>"[..],  // control character
        &b"bplist"[..],
        &b"bplist00"[..],
        &b"bplist01xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"[..],
        &b"bplist15xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"[..],
    ] {
        let e = Plist.extract(b, &ExtractOptions::default()).expect_err(&format!("{b:?}"));
        assert!(matches!(e, Error::Parse { .. } | Error::Encoding(_) | Error::Limit(_)), "{b:?}: {e:?}");
    }
    // a binary plist whose trailer lies
    let good = corpus_file("binary_info.plist");
    for (at, v) in [(good.len() - 26, 0u8), (good.len() - 26, 9), (good.len() - 25, 0), (good.len() - 25, 9), (good.len() - 9, 0xFF), (good.len() - 1, 0xFF), (good.len() - 17, 0xFF)] {
        let mut b = good.clone();
        b[at] = v;
        assert!(Plist.extract(&b, &ExtractOptions::default()).is_err(), "byte {at}={v}");
    }
}

#[test]
fn merge_rejects_a_broken_document() {
    let src = include_bytes!("data/Info.plist");
    let mut d = ex(F::Plist, src);
    if let Skeleton::Text { parts, .. } = &mut d.skeleton {
        parts.push(SkelPart::Unit(99));
    }
    assert!(matches!(Plist.merge(&d), Err(Error::Write(_))));
    let mut d = ex(F::Plist, src);
    if let Skeleton::Text { encoding, .. } = &mut d.skeleton {
        *encoding = "latin-9".into();
    }
    assert!(Plist.merge(&d).is_err());
    let mut d = ex(F::Plist, src);
    d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(Plist.merge(&d), Err(Error::Write(_))));
    // a unit whose hidden codes were stripped writes nothing for an untouched value but does not panic
    let mut d = ex(F::Plist, src);
    d.units[0].codes.clear();
    let _ = Plist.merge(&d);
}

#[test]
fn a_nul_or_control_character_in_a_translation_never_reaches_the_file() {
    let src = include_bytes!("data/Info.plist");
    let mut d = ex(F::Plist, src);
    set_nth(&mut d, 0, "a\0b\u{1}c\u{8}d");
    let out = String::from_utf8(merged(F::Plist, &d)).unwrap();
    assert!(out.contains("<string>abcd</string>"), "{out}");
    let mut b = ex(F::Plist, &corpus_file("binary_info.plist"));
    set_nth(&mut b, 0, "a\0b");
    assert_eq!(units_of(F::Plist, &merged(F::Plist, &b))[0], "ab");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1500))]

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..400)) {
        check(&b);
    }

    #[test]
    fn random_bytes_after_a_binary_header_never_panic(b in proptest::collection::vec(any::<u8>(), 0..300)) {
        let mut v = b"bplist00".to_vec();
        v.extend_from_slice(&b);
        check(&v);
    }

    #[test]
    fn random_text_never_panics(s in "[ -~\u{e9}\u{65e5}\n\r\t]{0,300}") {
        check(s.as_bytes());
        let mut u16 = vec![0xFF, 0xFE];
        for u in s.encode_utf16() { u16.extend_from_slice(&u.to_le_bytes()); }
        check(&u16);
    }

    #[test]
    fn structured_noise_never_panics(parts in proptest::collection::vec(prop_oneof![
        Just("<plist>".to_string()), Just("</plist>".to_string()), Just("<dict>".to_string()), Just("</dict>".to_string()),
        Just("<array>".to_string()), Just("</array>".to_string()), Just("<key>".to_string()), Just("</key>".to_string()),
        Just("<string>".to_string()), Just("</string>".to_string()), Just("<string/>".to_string()), Just("<key/>".to_string()),
        Just("<integer>".to_string()), Just("</integer>".to_string()), Just("<date>".to_string()), Just("<data>".to_string()),
        Just("<true/>".to_string()), Just("<![CDATA[".to_string()), Just("]]>".to_string()), Just("<!--".to_string()), Just("-->".to_string()),
        Just("<?xml version=\"1.0\"?>".to_string()), Just("<!DOCTYPE plist>".to_string()), Just("&amp;".to_string()), Just("&#".to_string()),
        Just("&".to_string()), Just(";".to_string()), Just("%@".to_string()), Just("%1$#@x@".to_string()), Just("NSStringPluralRuleType".to_string()),
        Just("NSStringFormatSpecTypeKey".to_string()), Just("other".to_string()), Just("one".to_string()), Just(" a='b' ".to_string()),
        Just("\n".to_string()), Just("\u{e9}".to_string()), Just("\u{1F600}".to_string()), Just("CFBundleName".to_string()),
    ], 0..70)) {
        check(parts.concat().as_bytes());
        check(format!("<plist><dict><key>CFBundleName</key>{}", parts.concat()).as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..13, edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0u8..3), 1..6)) {
        let mut b = fixtures()[idx].clone();
        for (pos, val, how) in edits {
            if b.is_empty() { break; }
            let p = pos.index(b.len());
            match how {
                0 => b[p] = val,
                1 => { b.remove(p); }
                _ => b.insert(p, val),
            }
        }
        check(&b);
    }
}

/// One dictionary of `n` distinct keys (objects `2..n+2`), every value the integer at object 1.
fn many_keys_binary(n: usize) -> Vec<u8> {
    let mut out = b"bplist00".to_vec();
    let mut offs: Vec<u32> = vec![out.len() as u32];
    out.extend_from_slice(&[0xDF, 0x12]);
    out.extend_from_slice(&(n as u32).to_be_bytes());
    for k in 0..n {
        out.extend_from_slice(&((k + 2) as u32).to_be_bytes());
    }
    for _ in 0..n {
        out.extend_from_slice(&1u32.to_be_bytes());
    }
    offs.push(out.len() as u32);
    out.extend_from_slice(&[0x10, 1]);
    for k in 0..n {
        offs.push(out.len() as u32);
        out.push(0x58);
        out.extend_from_slice(format!("k{k:07}").as_bytes());
    }
    let table = out.len() as u64;
    for o in &offs {
        out.extend_from_slice(&o.to_be_bytes());
    }
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&[4, 4]);
    out.extend_from_slice(&((n + 2) as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&table.to_be_bytes());
    out
}

#[test]
fn a_dict_of_many_distinct_keys_reads_in_linear_time() {
    // gate: a key lookup per entry made this quadratic (150 000 keys ran for minutes)
    let b = many_keys_binary(150_000);
    let t = std::time::Instant::now();
    let d = Plist.extract(&b, &ExtractOptions { include_locked: true, ..Default::default() }).unwrap();
    assert!(d.units.is_empty());
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
}

#[test]
fn binary_nesting_at_the_limit_fits_a_one_megabyte_stack() {
    // gate: 127 nested arrays overflowed a 1 MB main-thread stack in a debug build
    let h = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            assert!(Plist.extract(&deep_binary(127), &ExtractOptions::default()).is_ok());
            assert_eq!(limit_kind(&Plist.extract(&deep_binary(129), &ExtractOptions::default()).unwrap_err()), Some(LimitKind::XmlDepth));
        })
        .unwrap();
    h.join().unwrap();
}
