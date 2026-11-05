//! Hostile and malformed input: typed errors, never a panic. A property test over random bytes and byte-mutated
//! packages, plus hand-made hostile zips.

mod common;
use bilingua_model::{ExtractOptions, Inline};
use common::*;
use proptest::prelude::*;

fn try_all(data: &[u8]) {
    let f = filter();
    if let Ok(mut doc) = f.extract(data, &ExtractOptions { target_lang: Some("ar-SA".into()), ..Default::default() }) {
        let _ = f.merge(&doc);
        for u in &mut doc.units {
            let mut t = u.segments[0].source.clone();
            t.reverse();
            t.push(Inline::text(" x\r\n\u{1}"));
            u.segments[0].target = t;
        }
        let _ = f.merge(&doc);
    }
}

fn mutate(bytes: &mut Vec<u8>, seed: &[(usize, u8, u8)]) {
    for (pos, op, val) in seed {
        if bytes.is_empty() {
            return;
        }
        let p = pos % bytes.len();
        match op % 4 {
            0 => bytes[p] = *val,
            1 => {
                bytes.remove(p);
            }
            2 => bytes.insert(p, *val),
            _ => bytes.truncate(p.max(1)),
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        try_all(&bytes);
        let mut with_sig = b"PK\x03\x04".to_vec();
        with_sig.extend(&bytes);
        try_all(&with_sig);
    }

    #[test]
    fn mutated_packages_never_panic(which in 0usize..3, seed in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..8)) {
        let mut pkg = [xlz(), wsxz(), mqxlz()][which].clone();
        mutate(&mut pkg, &seed);
        try_all(&pkg);
    }

    #[test]
    fn mutated_members_never_panic(which in 0usize..3, seed in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..8)) {
        // the zip is valid, the XLIFF inside is not
        let mut x = [WS, SDL, MQ][which].to_vec();
        mutate(&mut x, &seed);
        try_all(&zip_of(&[("a.xlf", &x, true), ("b.bin", b"zz", false)], b"c"));
    }
}

#[test]
fn hostile_central_directory_offsets() {
    // an end-of-directory record that points past the file, and a directory entry with huge lengths
    let mut z = xlz();
    let n = z.len();
    z[n - 6..n - 2].copy_from_slice(&u32::MAX.to_le_bytes());
    try_all(&z);
    let mut z = xlz();
    for i in 0..z.len().saturating_sub(34) {
        if z[i..i + 4] == [0x50, 0x4b, 1, 2] {
            z[i + 28..i + 34].copy_from_slice(&[0xFF; 6]);
        }
    }
    try_all(&z);
    let mut z = xlz();
    let n = z.len();
    z[n - 12..n - 8].copy_from_slice(&u32::MAX.to_le_bytes());
    try_all(&z);
}

#[test]
fn deep_and_wide_xliff_members_give_errors() {
    let deep = format!("<?xml version=\"1.0\"?><xliff version=\"1.2\">{}{}</xliff>", "<g>".repeat(5000), "</g>".repeat(5000));
    try_all(&zip_of(&[("a.xlf", deep.as_bytes(), true)], b""));
    assert!(filter().extract(&zip_of(&[("a.xlf", deep.as_bytes(), true)], b""), &ExtractOptions::default()).is_err());
    let trunc = &WS[..WS.len() / 2];
    assert!(filter().extract(&zip_of(&[("a.xlf", trunc, true)], b""), &ExtractOptions::default()).is_err());
    // empty file, whitespace, only a BOM
    for body in [&b""[..], b"   ", b"\xEF\xBB\xBF", b"\xFF\xFE", b"\xFF\xFE\x00"] {
        assert!(filter().extract(&zip_of(&[("a.xlf", body, true)], b""), &ExtractOptions::default()).is_err());
    }
}

#[test]
fn xxe_and_entity_bombs_are_refused() {
    for dt in [
        "<!DOCTYPE x [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]>",
        "<!DOCTYPE x [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;&a;&a;\">]>",
    ] {
        let x = text(WS).replacen("<xliff ", &format!("{dt}<xliff "), 1);
        assert!(filter().extract(&zip_of(&[("a.xlf", x.as_bytes(), true)], b""), &ExtractOptions::default()).is_err());
        // the same inside a UTF-16 member
        let mut u: Vec<u8> = vec![0xFF, 0xFE];
        for c in x.encode_utf16() {
            u.extend(c.to_le_bytes());
        }
        assert!(filter().extract(&zip_of(&[("a.xlf", &u, true)], b""), &ExtractOptions::default()).is_err());
    }
}
