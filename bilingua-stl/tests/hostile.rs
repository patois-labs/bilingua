//! Hostile input: truncated, garbled and huge files give typed errors (or read), never a panic; property tests over
//! random bytes, byte-mutated fixtures and generated files.

mod common;
use bilingua_model::*;
use bilingua_stl::Stl;
use common::*;
use proptest::prelude::*;

fn fixture() -> Vec<u8> {
    let tf = |s: &[u8]| cat(&[DH, s, END]);
    let blocks = [
        t(1, &cat(&[&tf(b"Hello there,"), b"\x8a\x8a", &tf(b"how are you?")])),
        t(2, &tf(b"Caf\xc2e cr\xc8eme \x01red\x07 x")),
        tti(&Tti { sn: 3, cf: 1, ..Default::default() }, b"a comment"),
        tti(&Tti { sn: 4, ebn: 0x00, ..Default::default() }, &cat(&[DH, b"long ", END])),
        t(4, &cat(&[b"\x8a\x8a", DH, b"extension.", END])),
        tti(&Tti { sn: 5, ebn: 0xFE, ..Default::default() }, b"user data"),
        t(6, b"\x80Italic\x81 open text $5 #1"),
    ];
    let mut v = gsi_default(blocks.len());
    for b in &blocks {
        v.extend_from_slice(b);
    }
    v
}

fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_))
}

/// The invariants of any file that extracts: the untouched merge returns the bytes, and a pseudo-translated merge
/// either reads back with the same unit count or is refused with a typed error.
fn check_file(bytes: &[u8]) {
    for target in [None, Some("ru-RU"), Some("ja"), Some("fr")] {
        let mut d = match Stl.extract(bytes, &opts(target)) {
            Ok(d) => d,
            Err(e) => {
                assert!(is_typed(&e), "{e:?}");
                continue;
            }
        };
        assert_eq!(Stl.merge(&d).expect("merge of an extracted document"), bytes, "untouched merge changed the file");
        pseudo(&mut d);
        match Stl.merge_with(&d, &MergeOptions::locos()) {
            Ok(out) => {
                let back = Stl.extract(&out, &opts(None)).expect("the translated file must extract");
                assert_eq!(back.units.len(), d.units.len());
            }
            Err(Error::Write(_)) => {}
            Err(e) => panic!("untyped merge error {e:?}"),
        }
    }
    let _ = Stl.sniff(bytes);
}

fn mutate(base: &[u8], edits: &[(usize, u8, u8)]) -> Vec<u8> {
    let mut v = base.to_vec();
    for &(pos, byte, kind) in edits {
        if v.is_empty() {
            break;
        }
        let at = pos % v.len();
        match kind % 4 {
            0 => v[at] = byte,
            1 => v.insert(at, byte),
            2 => {
                v.remove(at);
            }
            _ => v.truncate(at),
        }
    }
    v
}

#[test]
fn fixtures_and_edges() {
    let f = fixture();
    check_file(&f);
    for n in 0..f.len().min(1024 + 128 * 3) {
        if n % 7 == 0 || !(40..=1000).contains(&n) {
            check_file(&f[..n]);
        }
    }
    check_file(&[]);
    check_file(&[0u8; 5000]);
    check_file(&gsi_default(0));
}

#[test]
fn more_than_99999_blocks_is_refused() {
    let mut v = gsi_default(0);
    v.resize(1024 + 128 * 100_000, 0x8F);
    // all-0x8F blocks: no text; the count alone refuses
    assert!(matches!(Stl.extract(&v, &opts(None)), Err(Error::Limit(_))));
}

#[test]
fn input_limit_and_unit_chars() {
    let f = fixture();
    let o = ExtractOptions { limits: Some(Limits { max_input_bytes: 1500, ..Default::default() }), ..Default::default() };
    assert!(matches!(Stl.extract(&f, &o), Err(Error::Limit(_))));
    let o = ExtractOptions { limits: Some(Limits { max_unit_chars: 5, ..Default::default() }), ..Default::default() };
    assert!(matches!(Stl.extract(&f, &o), Err(Error::Limit(_))));
}

#[test]
fn deep_extension_chain_and_many_rows_do_not_blow_up() {
    // one subtitle of 240 extension blocks of rows separated by 0x8A
    let mut v = Vec::new();
    for k in 0..240u32 {
        let ebn = if k == 239 { 0xFF } else { k as u8 };
        let tf: Vec<u8> = (0..56).flat_map(|_| [b'a', 0x8A]).collect();
        v.extend(tti(&Tti { sn: 1, ebn, ..Default::default() }, &tf));
    }
    let data = cat(&[&gsi_default(240), &v]);
    check_file(&data);
    let d = ex(&data);
    assert_eq!(d.units.len(), 1);
    // a translation that needs fewer rows still works, and an over-long one is refused quickly
    let big = vec![(0usize, "x ".repeat(5000))];
    let r = build(&data, &big);
    assert!(matches!(r, Ok(_) | Err(Error::Write(_))));
}

#[test]
fn hostile_translations_cannot_create_cues_codes_or_header_fields() {
    let data = fixture();
    let orig_blocks = blocks(&data).len();
    let nasty = [
        "line\u{8a}two",
        "ctl\u{1}red",
        "ctl\u{8f}end",
        "\u{27E6}x1\u{27E7} \u{27E6}x99\u{27E7} \u{27E6}x\u{27E7}",
        "a\n\n\n\n\n\nb",
        "a\rb",
        "STL25.01 850 header field",
        "\u{0}\u{0}\u{0}",
        "\u{feff}bom",
        "\u{202e}rtl",
        "tab\tand\u{b}vt",
    ];
    for s in nasty {
        for k in 0..ex(&data).units.len() {
            let r = build(&data, &[(k, s.to_string())]);
            let Ok(out) = r else { continue };
            // the GSI changes only in CCT (12-13) and TNB (238-243); no subtitle was added
            let mut a = data[..1024].to_vec();
            let mut b = out[..1024].to_vec();
            for r in [12..14, 238..243] {
                for i in r {
                    a[i] = 0;
                    b[i] = 0;
                }
            }
            assert_eq!(a, b, "{s:?}: header changed");
            let back = Stl.extract(&out, &opts(None)).unwrap();
            assert_eq!(back.units.len(), ex(&data).units.len(), "{s:?}: unit count");
            // the number of subtitles (distinct SN among text blocks) is the same
            let sns = |d: &[u8]| {
                let mut v: Vec<[u8; 2]> = blocks(d).iter().map(|b| [b[1], b[2]]).collect();
                v.dedup();
                v
            };
            assert_eq!(sns(&out), sns(&data), "{s:?}: cues");
            assert!(blocks(&out).len() >= orig_blocks - 1);
            // control bytes in the output text fields are only ones the original subtitle had
            let had: std::collections::BTreeSet<u8> = blocks(&data).iter().flat_map(|b| b[16..].iter().copied()).filter(|c| *c < 0x20 || (0x80..0xA0).contains(c)).collect();
            for blk in blocks(&out) {
                for c in &blk[16..] {
                    if *c < 0x20 || (0x80..0xA0).contains(c) {
                        assert!(had.contains(c) || *c == 0x8F || *c == 0x8A, "{s:?}: new control byte {c:#x}");
                    }
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..3000)) {
        check_file(&bytes);
    }

    #[test]
    fn random_blocks_behind_a_valid_gsi_never_panic(body in proptest::collection::vec(any::<u8>(), 0..1500), cct in 0u8..6, n in 0usize..8) {
        let mut v = gsi(&[b'0', b'0' + cct], b"09", b'1', b"40", 0);
        // whole blocks, with a few of them given a text-like sn/ebn so groups form
        let mut body = body;
        body.resize(n * 128, 0x8F);
        v.extend(body);
        check_file(&v);
    }

    #[test]
    fn mutated_fixture_never_panics(edits in proptest::collection::vec((0usize..100_000, any::<u8>(), any::<u8>()), 1..12)) {
        check_file(&mutate(&fixture(), &edits));
    }

    #[test]
    fn mutated_text_fields_never_panic(edits in proptest::collection::vec((0usize..7, 16usize..128, any::<u8>()), 1..30)) {
        let mut v = fixture();
        for (blk, off, byte) in edits {
            v[1024 + blk * 128 + off] = byte;
        }
        check_file(&v);
    }

    #[test]
    fn random_translations_never_panic(text in ".{0,300}", k in 0usize..3, cct in 0u8..5) {
        let mut v = gsi(&[b'0', b'0' + cct], b"09", b'1', b"40", 2);
        v.extend(t(1, &cat(&[DH, b"Hello there,", END, b"\x8a\x8a", DH, b"how are you?", END])));
        v.extend(t(2, b"Second \x01one\x07"));
        let mut d = ex(&v);
        let kk = k % d.units.len();
        set(&mut d, kk, &text);
        match Stl.merge_with(&d, &MergeOptions::locos()) {
            Ok(out) => { prop_assert!(Stl.extract(&out, &opts(None)).is_ok()); }
            Err(Error::Write(_)) => {}
            Err(e) => prop_assert!(false, "untyped {e:?}"),
        }
    }
}
