//! Hostile input: every offset and length in an MO file is untrusted. Truncated, overlapping, self-referencing and
//! huge-count files give typed errors (or read), never a panic and never a wild allocation; property tests over
//! random bytes, byte-mutated fixtures, word-mutated fixtures and generated catalogs.

mod common;
use bilingua_model::*;
use bilingua_mo::Mo;
use common::*;
use proptest::prelude::*;

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

fn fixtures() -> Vec<Vec<u8>> {
    let mut names: Vec<String> = std::fs::read_dir(DATA)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".mo"))
        .collect();
    names.sort();
    names.iter().map(|n| std::fs::read(format!("{DATA}/{n}")).unwrap()).collect()
}

fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_))
}

/// The invariants of any file that extracts: the untouched merge returns the bytes, and a pseudo-translated merge
/// reads back (with no more units than before: a duplicate key is dropped, as msgfmt does).
fn check_file(bytes: &[u8]) {
    for target in [None, Some("ru-RU"), Some("ja")] {
        let r = Mo.extract(bytes, &opts(target));
        let mut d = match r {
            Ok(d) => d,
            Err(e) => {
                assert!(is_typed(&e), "{e:?}");
                continue;
            }
        };
        assert_eq!(Mo.merge(&d).expect("merge of an extracted document"), bytes, "untouched merge changed the file");
        pseudo(&mut d);
        let out = Mo.merge(&d).expect("merge of a translated document");
        let back = Mo.extract(&out, &opts(None)).expect("the translated catalog must extract");
        assert!(back.units.len() <= d.units.len());
    }
    let _ = Mo.sniff(bytes);
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

const NASTY: [u32; 10] = [0, 1, 3, 4, 27, 28, 0x7fff_ffff, 0x8000_0000, 0xffff_fffe, 0xffff_ffff];

fn word_edit(base: &[u8], edits: &[(usize, usize, bool)]) -> Vec<u8> {
    let mut v = base.to_vec();
    let len = v.len() as u32;
    for &(pos, pick, big) in edits {
        if v.len() < 8 {
            break;
        }
        // mostly the header and the first table entries, where the counts and offsets are
        let at = (pos % (v.len().min(160) / 4)) * 4;
        let w = match pick % 14 {
            n if n < 10 => NASTY[n],
            10 => len,
            11 => len.wrapping_sub(1),
            12 => len / 2,
            _ => (pick as u32).wrapping_mul(2_654_435_761),
        };
        let b = if big { w.to_be_bytes() } else { w.to_le_bytes() };
        v[at..at + 4].copy_from_slice(&b);
    }
    v
}

#[test]
fn every_truncation_is_handled() {
    for (i, f) in fixtures().iter().enumerate() {
        for cut in 0..f.len() {
            match try_ex(&f[..cut]) {
                Ok(_) => {}
                Err(e) => assert!(is_typed(&e), "fixture {i} cut at {cut}: {e:?}"),
            }
        }
        check_file(f);
    }
}

#[test]
fn every_header_word_set_to_nasty_values() {
    let base = mo_le();
    for word in 0..7 {
        for w in NASTY.iter().chain(&[base.len() as u32, base.len() as u32 - 1]) {
            let mut v = base.clone();
            v[4 * word..4 * word + 4].copy_from_slice(&w.to_le_bytes());
            check_file(&v);
            let mut b = mo_be_hash();
            b[4 * word..4 * word + 4].copy_from_slice(&w.to_be_bytes());
            check_file(&b);
        }
    }
    // every table word too
    for at in (28..28 + 16 * 5).step_by(4) {
        for w in NASTY.iter().chain(&[base.len() as u32, base.len() as u32 - 1]) {
            let mut v = base.clone();
            v[at..at + 4].copy_from_slice(&w.to_le_bytes());
            check_file(&v);
        }
    }
}

#[test]
fn self_referencing_tables_do_not_loop_or_explode() {
    // Strings that point at the header, at the tables, at each other, and at the same bytes many times.
    let n = 1000u32;
    let mut f = Vec::new();
    for w in [0x9504_12de_u32, 0, n, 28, 28 + 8 * n, 0, 0] {
        f.extend_from_slice(&w.to_le_bytes());
    }
    for _ in 0..2 {
        for _ in 0..n {
            f.extend_from_slice(&20u32.to_le_bytes());
            f.extend_from_slice(&0u32.to_le_bytes());
        }
    }
    f.extend_from_slice(&[0u8; 64]);
    match try_ex(&f) {
        Ok(_) => {}
        Err(e) => assert!(is_typed(&e), "{e:?}"),
    }
    // Overlap is capped by the file size: 50 000 strings of 4 KB inside a 400 KB file are refused.
    let n = 50_000u32;
    let size = 28 + 16 * n as usize + 4096;
    let mut f = Vec::new();
    for w in [0x9504_12de_u32, 0, n, 28, 28 + 8 * n, 0, 0] {
        f.extend_from_slice(&w.to_le_bytes());
    }
    let base = 28 + 16 * n;
    for _ in 0..2 {
        for _ in 0..n {
            f.extend_from_slice(&4000u32.to_le_bytes());
            f.extend_from_slice(&base.to_le_bytes());
        }
    }
    f.resize(size, b'a');
    let e = try_ex(&f).unwrap_err();
    assert!(is_typed(&e) && e.to_string().contains("overlapping"), "{e}");
}

#[test]
fn huge_counts_are_refused_before_allocation() {
    for n in [0xffff_ffffu32, 0x8000_0000, 1 << 28, 5_000_000, 50_001] {
        let mut f = mo_le();
        f[8..12].copy_from_slice(&n.to_le_bytes());
        let e = try_ex(&f).unwrap_err();
        assert!(is_typed(&e), "{n}: {e:?}");
    }
    // a lowered cap is honoured
    let lim = Limits::new().with_max_units(2);
    assert!(Mo.extract(&mo_le(), &ExtractOptions::new().with_limits(lim)).is_err());
    // the input size limit comes first
    let lim = Limits::new().with_max_input_bytes(10);
    let e = Mo.extract(&mo_le(), &ExtractOptions::new().with_limits(lim)).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::InputBytes), "{e:?}");
    // a string longer than the unit limit allows (8 bytes a character)
    let lim = Limits::new().with_max_unit_chars(2);
    assert!(matches!(Mo.extract(&mo_le(), &ExtractOptions::new().with_limits(lim)), Err(Error::Limit(_))));
}

#[test]
fn a_unit_longer_than_the_limit_is_refused() {
    let long = "x".repeat(300);
    let f = pack(&pairs(&[("", HEADER_FR), (long.as_str(), "y")]));
    let lim = Limits::new().with_max_unit_chars(200);
    let e = Mo.extract(&f, &ExtractOptions::new().with_limits(lim)).unwrap_err();
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::UnitChars), "{e:?}");
}

#[test]
fn bad_charsets_and_strings_are_typed_errors() {
    for data in [
        pack(&[(b"".to_vec(), b"Content-Type: text/plain; charset=UTF-8\n".to_vec()), (b"A".to_vec(), b"\xff".to_vec())]),
        pack(&[(b"".to_vec(), b"Content-Type: text/plain; charset=us-ascii\n".to_vec()), (b"A".to_vec(), "\u{e9}".as_bytes().to_vec())]),
        pack(&[(b"".to_vec(), b"Content-Type: text/plain; charset=shift_jis\n".to_vec()), (b"A".to_vec(), b"\x81".to_vec())]),
        pack(&[(b"".to_vec(), b"Content-Type: text/plain; charset=windows-1252\n".to_vec()), (b"A".to_vec(), b"\x81".to_vec())]),
        pack(&[(b"".to_vec(), HEADER_FR.as_bytes().to_vec()), (b"a\x04b\x04c\0d\0e".to_vec(), b"x".to_vec())]),
    ] {
        let e = try_ex(&data).unwrap_err();
        assert!(is_typed(&e), "{e:?}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(3000))]

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..1500)) {
        check_file(&b);
    }

    #[test]
    fn random_bytes_behind_a_valid_magic_never_panic(big in any::<bool>(), b in proptest::collection::vec(any::<u8>(), 0..800)) {
        let mut v = if big { 0xde12_0495u32.to_le_bytes().to_vec() } else { 0x9504_12deu32.to_le_bytes().to_vec() };
        v.extend(b);
        check_file(&v);
    }

    #[test]
    fn mutated_fixtures_never_panic(
        which in 0usize..64,
        edits in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 0..10),
    ) {
        let all = fixtures();
        check_file(&mutate(&all[which % all.len()], &edits));
    }

    #[test]
    fn word_mutated_fixtures_never_panic(
        which in 0usize..64,
        edits in proptest::collection::vec((any::<usize>(), any::<usize>(), any::<bool>()), 1..6),
    ) {
        let all = fixtures();
        check_file(&word_edit(&all[which % all.len()], &edits));
    }
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z %d{}()$0-9.,!?-]{1,24}",
        any::<String>().prop_map(|s| format!("a{}", s.replace(['\0', '\u{4}'], "").chars().take(40).collect::<String>())),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// A generated catalog (any order, any byte order, a hash table or none, contexts, plurals) extracts, comes
    /// back byte for byte, and a translation merged in reads back as exactly what was written.
    #[test]
    fn generated_catalogs_translate_exactly(
        entries in proptest::collection::btree_map(text(), (proptest::option::of("[a-z]{1,6}"), proptest::option::of(text()), proptest::collection::vec(text(), 1..4)), 0..10),
        big in any::<bool>(),
        hash in 0u32..9,
        translation in text(),
    ) {
        let mut p: Vec<(Vec<u8>, Vec<u8>)> = vec![(Vec::new(), HEADER_FR.as_bytes().to_vec())];
        let mut expect_units = 0;
        for (id, (ctx, plural, strs)) in &entries {
            let mut key = id.clone();
            if let Some(pl) = plural {
                key = format!("{key}\0{pl}");
            }
            if let Some(c) = ctx {
                key = format!("{c}\u{4}{key}");
            }
            let val = if plural.is_some() { strs.join("\0") } else { strs[0].clone() };
            p.push((key.into_bytes(), val.into_bytes()));
            expect_units += if plural.as_ref().is_some_and(|x| !x.is_empty()) { 2 } else { 1 };
        }
        p.sort();
        let bytes = pack_with(&p, &Pack { big, hash_size: hash, ..Default::default() });
        let mut d = Mo.extract(&bytes, &opts(None)).expect("a well-formed catalog");
        prop_assert_eq!(d.units.len(), expect_units);
        prop_assert_eq!(Mo.merge(&d).unwrap(), bytes);
        // translate the first unit
        if let Some(first) = d.units.first().map(|u| u.id.clone()) {
            set(&mut d, &first, &format!("<{translation}>"));
            let out = Mo.merge(&d).unwrap();
            let back = Mo.extract(&out, &opts(None)).unwrap();
            let want = format!("<{translation}>");
            prop_assert!(back.units.iter().any(|u| flat_target_of(u) == want));
        }
    }
}

fn flat_target_of(u: &Unit) -> String {
    u.segments[0].target.iter().map(|i| match i { Inline::Text(t) => t.clone(), Inline::Tag { display, .. } => display.clone() }).collect()
}
